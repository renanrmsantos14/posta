// Android edge-to-edge: a WebView devolve 0 em env(safe-area-inset-*). A MainActivity expõe as barras do sistema
// (e o teclado, em `bottom`) por `window.PostaInsets` e avisa mudanças com o evento "posta-insets".
// Aqui viram as variáveis --sa-top/--sa-right/--sa-bottom/--sa-left; fora do Android vale o env() do styles.css.

declare global {
  interface Window {
    PostaInsets?: { get(): string };
  }
}

export {};

const SIDES = ["top", "right", "bottom", "left"] as const;

function apply() {
  let raw: Record<string, unknown>;
  try {
    raw = JSON.parse(window.PostaInsets?.get() || "null") ?? {};
  } catch {
    return;
  }
  for (const side of SIDES) {
    const v = raw[side];
    if (typeof v === "number" && Number.isFinite(v)) document.documentElement.style.setProperty(`--sa-${side}`, `${v}px`);
  }
}

if (window.PostaInsets) {
  apply();
  window.addEventListener("posta-insets", apply);
}
