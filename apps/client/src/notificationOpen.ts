// Clique numa notificação de email novo: o destino (conta, pasta e, se for um só email, o id) chega por três caminhos.
// Desktop e Android com o app vivo: o backend guarda o pedido (`take_pending_open`) e emite "open-target".
// Android aberto pela notificação: o Intent vem pela ponte `window.PostaNative` e o evento "posta-open" (MainActivity.kt).
// No mock, "posta-open" com `detail` simula o clique.
import { invoke } from "@tauri-apps/api/core";
import { isTauri, type View } from "./api";

/** Espelha `OpenTarget` em src-tauri/src/open_target.rs; sem `id` = só a pasta. */
export interface OpenTarget {
  account: string;
  folder: View;
  id?: number;
}

declare global {
  interface Window {
    /** Só no Android: devolve o JSON do destino do toque, uma vez ("" se não há). */
    PostaNative?: { takePending(): string };
  }
}

/** Valida um payload vindo de fora (Rust, Intent ou teste): conta e pasta obrigatórias, id numérico opcional. */
export function parseOpenTarget(raw: unknown): OpenTarget | null {
  if (!raw || typeof raw !== "object") return null;
  const { account, folder, id } = raw as Record<string, unknown>;
  if (typeof account !== "string" || !account || typeof folder !== "string" || !folder) return null;
  if (id != null && !Number.isSafeInteger(id)) return null;
  return { account, folder: folder as View, ...(typeof id === "number" ? { id } : {}) };
}

async function takePending(): Promise<OpenTarget | null> {
  try {
    const native = window.PostaNative?.takePending();
    if (native) return parseOpenTarget(JSON.parse(native));
    return isTauri ? parseOpenTarget(await invoke("take_pending_open")) : null;
  } catch {
    return null;
  }
}

/** Entrega `run` o destino pendente agora (app iniciado pelo clique) e a cada novo clique; devolve como desligar. */
export function watchOpenTarget(run: (t: OpenTarget) => void): () => void {
  let alive = true;
  const drain = () => void takePending().then((t) => alive && t && run(t));
  const onWindow = (e: Event) => {
    const detail = (e as CustomEvent).detail;
    if (detail) {
      const t = parseOpenTarget(detail);
      if (t) run(t);
    } else drain();
  };
  window.addEventListener("posta-open", onWindow);
  let off: (() => void) | undefined;
  if (isTauri) {
    import("@tauri-apps/api/event")
      .then(({ listen }) => listen("open-target", drain))
      .then((un) => (alive ? (off = un) : un()));
  }
  drain();
  return () => {
    alive = false;
    window.removeEventListener("posta-open", onWindow);
    off?.();
  };
}
