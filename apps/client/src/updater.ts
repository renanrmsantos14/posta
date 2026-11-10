// Atualização pelo próprio app: procura no latest.json do GitHub Releases, baixa, instala e reinicia.
// Fora do Tauri (navegador/mocks), ?update na URL simula uma versão nova para testar a interface.
import { isTauri } from "./api";
import build from "./version.json";

/** Novidades de uma versão: os commits que entraram nela. */
export interface Release { version: string; changes: string[] }

export interface Update {
  version: string;
  /** Todas as versões entre a instalada e a nova, da mais recente para a mais antiga. */
  releases: Release[];
  /** Só no Android: URL do APK do release; o app abre no navegador/Android, que baixa e pede para instalar. */
  apkUrl?: string;
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

const REPO_API = "https://api.github.com/repos/renanrmsantos14/posta";
const LATEST_RELEASE = `${REPO_API}/releases/latest`;
/** No Android não há o atualizador do desktop: o release do GitHub traz um APK, que o sistema baixa e instala por cima. */
const isAndroid = isTauri && /Android/i.test(navigator.userAgent);

export const autoCheck = () => read(AUTO_KEY) !== "off";
export const setAutoCheck = (on: boolean) => write(AUTO_KEY, on ? "on" : "off");
/** "Depois": a checagem automática não insiste nessa versão; a manual mostra de novo. */
export const skipped = () => read(SKIP_KEY);
export const skip = (version: string) => write(SKIP_KEY, version);

export async function checkUpdate(): Promise<Update | null> {
  if (!isTauri) return mockCheck();
  if (isAndroid) return checkApk();
  const { check } = await import("@tauri-apps/plugin-updater");
  const found = await check();
  if (!found) return null;
  return {
    version: found.version,
    releases: await releasesUpTo(found.version),
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

/** true se a < b, comparando major.minor.patch ("v" opcional). */
function older(a: string, b: string) {
  const n = (v: string) => v.replace(/^v/, "").split(".").map((x) => parseInt(x, 10) || 0);
  const [x, y] = [n(a), n(b)];
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) < (y[i] ?? 0);
  return false;
}

/** O corpo do release só traz instruções de instalação: as novidades vêm dos commits entre as tags.
 *  Cada "Lança vX" fecha o grupo da versão X. Falha de rede não impede atualizar: só não lista. */
async function releasesUpTo(target: string): Promise<Release[]> {
  try {
    const res = await fetch(`${REPO_API}/compare/v${build.version}...v${target}`, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) return [];
    const { commits = [] }: { commits?: { parents: unknown[]; commit: { message: string } }[] } = await res.json();
    const out: Release[] = [];
    let pending: string[] = [];
    for (const c of commits) {
      if (c.parents.length > 1) continue; // merge de PR: os commits dele já estão na lista
      const subject = c.commit.message.split("\n")[0].trim();
      const launch = /^Lança v(\S+)/.exec(subject);
      if (!launch) { pending.push(subject); continue; }
      out.unshift({ version: launch[1], changes: pending });
      pending = [];
    }
    if (pending.length) out.unshift({ version: target, changes: pending });
    return out.filter((r) => r.changes.length);
  } catch {
    return [];
  }
}

async function openUrl(url: string) {
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

async function checkApk(): Promise<Update | null> {
  const res = await fetch(LATEST_RELEASE, { headers: { Accept: "application/vnd.github+json" } });
  if (!res.ok) throw new Error(`GitHub respondeu ${res.status}`);
  const rel: { tag_name: string; body?: string; assets?: { name: string; browser_download_url: string }[] } = await res.json();
  if (!older(build.version, rel.tag_name)) return null;
  // O APK entra no release alguns minutos depois dos instaladores; sem ele, ainda não há o que baixar.
  const apk = rel.assets?.find((a) => a.name.endsWith(".apk"));
  if (!apk) return null;
  const version = rel.tag_name.replace(/^v/, "");
  return {
    version,
    releases: await releasesUpTo(version),
    apkUrl: apk.browser_download_url,
    install: async () => openUrl(apk.browser_download_url),
  };
}

export async function restart() {
  if (!isTauri) return location.reload();
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

async function mockCheck(): Promise<Update | null> {
  await new Promise((r) => setTimeout(r, 700));
  const q = new URLSearchParams(location.search);
  if (!q.has("update")) return null;
  // ?update=android simula o aviso do Android (APK) no navegador.
  if (q.get("update") === "android") {
    return { version: "0.1.2", releases: [], apkUrl: "https://example.invalid/Posta_0.1.2_android.apk", install: async () => {} };
  }
  return {
    version: "0.1.1",
    releases: [
      { version: "0.1.1", changes: ["Busca mais rápida em caixas grandes", "Responder mantém a formatação da citação", "Correção: contador de não lidas após sincronizar"] },
      { version: "0.1.0", changes: ["Libera imagens remotas por remetente", "Acelera a sincronização inicial"] },
    ],
    install: async (progress) => {
      for (let p = 0; p <= 1; p += 0.05) {
        progress(p);
        await new Promise((r) => setTimeout(r, 120));
      }
    },
  };
}
