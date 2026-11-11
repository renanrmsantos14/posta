//! Senhas das contas. Desktop: cofre do sistema (crate `keyring`). Android: Android Keystore
//! (crate `android-native-keyring-store`, que cifra um SharedPreferences com chave do Keystore).

const SERVICE: &str = "posta";

#[cfg(not(target_os = "android"))]
mod imp {
    use super::SERVICE;

    /// O Gerenciador de Credenciais do Windows aceita até 2560 bytes por segredo, gravado em UTF-16
    /// (1280 unidades). Refresh tokens da Microsoft passam disso, então valores grandes são
    /// divididos em partes `id#1..id#n` e a entrada `id` guarda só o marcador com a contagem.
    /// Nos outros sistemas não há esse limite e o valor fica numa entrada só.
    #[cfg(windows)]
    const CHUNK_UTF16: usize = 1000;
    #[cfg(not(windows))]
    const CHUNK_UTF16: usize = usize::MAX;
    const MARKER: &str = "posta-partes:";

    fn entry(id: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())
    }

    fn part_id(id: &str, i: usize) -> String {
        format!("{id}#{i}")
    }

    fn raw_set(id: &str, value: &str) -> Result<(), String> {
        entry(id)?.set_password(value).map_err(|e| e.to_string())
    }

    fn raw_get(id: &str) -> Result<Option<String>, String> {
        match entry(id)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn raw_delete(id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn part_count(stored: &str) -> Option<usize> {
        stored.strip_prefix(MARKER)?.parse().ok()
    }

    /// Quantas partes o valor gravado em `id` usa hoje (0 = valor numa entrada só ou nenhum).
    fn stored_parts(id: &str) -> Result<usize, String> {
        Ok(raw_get(id)?.as_deref().and_then(part_count).unwrap_or(0))
    }

    /// Divide `value` em pedaços de no máximo `max` unidades UTF-16, sem quebrar caractere.
    pub(super) fn split(value: &str, max: usize) -> Vec<String> {
        let mut parts = vec![String::new()];
        let mut units = 0;
        for c in value.chars() {
            if units + c.len_utf16() > max {
                parts.push(String::new());
                units = 0;
            }
            units += c.len_utf16();
            parts.last_mut().unwrap().push(c);
        }
        parts
    }

    /// Grava o valor novo antes de mexer no antigo: se a gravação falhar, o segredo anterior segue legível.
    pub fn set(id: &str, password: &str) -> Result<(), String> {
        let old = stored_parts(id)?;
        let parts = split(password, CHUNK_UTF16);
        let new = if parts.len() == 1 {
            raw_set(id, password)?;
            0
        } else {
            for (i, part) in parts.iter().enumerate() {
                raw_set(&part_id(id, i + 1), part)?;
            }
            raw_set(id, &format!("{MARKER}{}", parts.len()))?;
            parts.len()
        };
        // Partes que sobraram do valor anterior ficam órfãs e inofensivas: falha ao apagá-las não desfaz a gravação.
        for i in new + 1..=old {
            let _ = raw_delete(&part_id(id, i));
        }
        Ok(())
    }

    pub fn get(id: &str) -> Result<String, String> {
        let stored = entry(id)?.get_password().map_err(|e| e.to_string())?;
        let Some(n) = part_count(&stored) else { return Ok(stored) };
        (1..=n).map(|i| entry(&part_id(id, i))?.get_password().map_err(|e| e.to_string())).collect()
    }

    /// Entrada que não existe não é erro: o objetivo (não ter segredo guardado) já está cumprido.
    pub fn delete(id: &str) -> Result<(), String> {
        for i in 1..=stored_parts(id)? {
            raw_delete(&part_id(id, i))?;
        }
        raw_delete(id)
    }
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::imp::split;

    #[test]
    fn split_respeita_limite_utf16_sem_quebrar_caractere() {
        assert_eq!(split("", 4), vec![""]);
        assert_eq!(split("abcd", 4), vec!["abcd"]);
        assert_eq!(split("abcdefghi", 4), vec!["abcd", "efgh", "i"]);
        // Emoji ocupa 2 unidades UTF-16: não cabe junto de 3 letras num pedaço de 4.
        assert_eq!(split("abc😀d", 4), vec!["abc", "😀d"]);
        assert_eq!(split(&"x".repeat(2500), 1000).len(), 3);
    }
}

#[cfg(target_os = "android")]
mod imp {
    use super::SERVICE;
    use keyring_core::Entry;
    use std::sync::OnceLock;

    /// Registra o cofre uma vez. Precisa do `ndk-context` só aqui: o Store guarda a JavaVM e o contexto.
    pub fn init() -> Result<(), String> {
        static READY: OnceLock<Result<(), String>> = OnceLock::new();
        READY
            .get_or_init(|| {
                let store = android_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
                keyring_core::set_default_store(store);
                Ok(())
            })
            .clone()
    }

    fn entry(id: &str) -> Result<Entry, String> {
        init()?;
        Entry::new(SERVICE, id).map_err(|e| e.to_string())
    }

    pub fn set(id: &str, password: &str) -> Result<(), String> {
        entry(id)?.set_password(password).map_err(|e| e.to_string())
    }

    pub fn get(id: &str) -> Result<String, String> {
        entry(id)?.get_password().map_err(|e| e.to_string())
    }

    pub fn delete(id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "android")]
pub use imp::init;
pub use imp::{delete, get, set};
