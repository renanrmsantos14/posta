//! Diagnóstico da abertura: etapas e travas vão para o logcat (tag "Posta") no Android e para o stderr no desktop.
//! Ler no celular: `adb logcat -s Posta PostaUI`.

use std::{sync::{mpsc, Mutex}, time::{Duration, Instant}};
use tauri::{ipc::Invoke, AppHandle, Runtime};

#[cfg(target_os = "android")]
#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}

/// Arquivo de log do desktop (`<dados do app>/logs/posta.log`). O release no Windows não tem console, então sem ele
/// um pânico ou uma saída não deixam rastro.
#[cfg(not(target_os = "android"))]
static LOG_FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Liga o log em arquivo. Passa de ~1 MB, o antigo vira `posta.log.old` (guarda só uma geração).
#[cfg(not(target_os = "android"))]
pub fn init_file(data_dir: &std::path::Path, version: &str) {
    let dir = data_dir.join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join("posta.log");
    if std::fs::metadata(&file).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        std::fs::rename(&file, dir.join("posta.log.old")).ok();
    }
    LOG_FILE.set(file).ok();
    log(&format!("--- Posta {version} iniciado (pid {}) ---", std::process::id()));
}

/// Registra pânicos (thread, local e mensagem) antes de o processo seguir ou cair.
#[cfg(not(target_os = "android"))]
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log(&format!("PÂNICO na thread {:?}: {info}", thread.name().unwrap_or("?")));
        default(info);
    }));
}

#[cfg(not(target_os = "android"))]
fn append(line: &str) {
    use std::io::Write;
    let Some(path) = LOG_FILE.get() else { return };
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        writeln!(f, "{secs} {line}").ok();
    }
}

pub fn log(msg: &str) {
    #[cfg(target_os = "android")]
    if let Ok(text) = std::ffi::CString::new(msg) {
        // 4 = ANDROID_LOG_INFO
        unsafe { __android_log_write(4, c"Posta".as_ptr(), text.as_ptr()) };
    }
    #[cfg(not(target_os = "android"))]
    {
        eprintln!("[posta] {msg}");
        append(msg);
    }
}

/// Comandos síncronos em execução: (seq, nome, início). O vigia avisa quando algum passa do limite.
static RUNNING: Mutex<Vec<(u64, String, Instant)>> = Mutex::new(Vec::new());
const SLOW: Duration = Duration::from_millis(500);

fn running() -> std::sync::MutexGuard<'static, Vec<(u64, String, Instant)>> {
    RUNNING.lock().unwrap_or_else(|p| p.into_inner())
}

/// Envolve o despacho de comandos. Os síncronos rodam dentro de `handler` (no Android, numa thread da WebView, não no
/// laço de eventos), então o tempo medido é o deles e o vigia vê qual está preso.
pub fn timed<R: Runtime>(handler: impl Fn(Invoke<R>) -> bool + Send + Sync + 'static) -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    move |invoke| {
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let start = Instant::now();
        running().push((seq, invoke.message.command().to_owned(), start));
        let _done = Done(seq);
        handler(invoke)
    }
}

/// Tira o comando da lista ao sair de `handler`, mesmo em pânico (senão o vigia o daria como preso para sempre).
struct Done(u64);

impl Drop for Done {
    fn drop(&mut self) {
        let mut list = running();
        let Some(i) = list.iter().position(|e| e.0 == self.0) else { return };
        let (_, cmd, start) = list.swap_remove(i);
        drop(list);
        let took = start.elapsed();
        if took >= SLOW {
            log(&format!("comando {cmd} levou {} ms", took.as_millis()));
        }
    }
}

/// A cada 2 s: avisa comandos presos e testa se o laço de eventos responde.
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        // Monta os avisos antes de escrever: o log fora da trava não segura o despacho de comandos.
        let stuck: Vec<String> = running().iter()
            .filter_map(|(_, cmd, start)| {
                let secs = start.elapsed().as_secs();
                (secs >= 3).then(|| format!("comando {cmd} em execução há {secs} s"))
            })
            .collect();
        stuck.iter().for_each(|m| log(m));
        let (tx, rx) = mpsc::channel();
        if app.run_on_main_thread(move || { tx.send(()).ok(); }).is_err() {
            return; // laço encerrado: o app está saindo
        }
        if rx.recv_timeout(Duration::from_secs(3)).is_err() {
            log("laço de eventos sem resposta há 3 s");
        }
    });
}
