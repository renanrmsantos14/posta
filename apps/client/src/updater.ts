// Atualização pelo próprio app: procura no latest.json do GitHub Releases, baixa, instala e reinicia.
// Fora do Tauri (navegador/mocks), ?update na URL simula uma versão nova para testar a interface.
import { isTauri } from "./api";

export interface Update {
  version: string;
  notes: string;
  /** Baixa e instala; progress recebe 0..1, ou null enquanto o tamanho é desconhecido. */
  install: (progress: (p: number | null) => void) => Promise<void>;
}

export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
const AUTO_KEY = "posta.update.auto";
const SKIP_KEY = "posta.update.skip";

function read(key: string) {
  try { return localStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* armazenamento indisponível: só não lembra */ }
}

/** No Android o APK é reinstalado à mão (o atualizador só existe no desktop). */
export const canUpdate = !(isTauri && /Android/i.test(navigator.userAgent));

export const autoCheck = () => canUpdate && read(AUTO_KEY) !== "off";
export const setAutoCheck = (on: boolean) => write(AUTO_KEY, on ? "on" : "off");
/** "Depois": a checagem automática não insiste nessa versão; a manual mostra de novo. */
export const skipped = () => read(SKIP_KEY);
export const skip = (version: string) => write(SKIP_KEY, version);

export async function checkUpdate(): Promise<Update | null> {
  if (!canUpdate) return null;
  if (!isTauri) return mockCheck();
  const { check } = await import("@tauri-apps/plugin-updater");
  const found = await check();
  if (!found) return null;
  return {
    version: found.version,
    notes: found.body?.trim() ?? "",
    install: async (progress) => {
      let total = 0;
      let done = 0;
      await found.downloadAndInstall((e) => {
        if (e.event === "Started") { total = e.data.contentLength ?? 0; progress(total ? 0 : null); }
        else if (e.event === "Progress") { done += e.data.chunkLength; if (total) progress(Math.min(done / total, 1)); }
        else progress(1);
      });
    },
  };
}

export async function restart() {
  if (!isTauri) return location.reload();
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

async function mockCheck(): Promise<Update | null> {
  await new Promise((r) => setTimeout(r, 700));
  if (!new URLSearchParams(location.search).has("update")) return null;
  return {
    version: "0.1.1",
    notes: "- Busca mais rápida em caixas grandes\n- Responder mantém a formatação da citação\n- Correção: contador de não lidas após sincronizar",
    install: async (progress) => {
      for (let p = 0; p <= 1; p += 0.05) {
        progress(p);
        await new Promise((r) => setTimeout(r, 120));
      }
    },
  };
}
