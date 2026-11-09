// Aparência: presets de tema (cores em themes.css), cor de destaque própria e espelho em localStorage para o
// public/theme-init.js aplicar tudo antes do primeiro paint. Sem dependências, para o App e as Configurações usarem.

export interface ThemeInfo {
  id: string;
  name: string;
  dark: boolean;
  /** Alto contraste: texto AAA (7:1); o destaque próprio também é ajustado a 7:1. */
  hc?: boolean;
}

/** Ordem da grade em Configurações. `light` e `dark` são o visual original. */
export const THEMES: ThemeInfo[] = [
  { id: "light", name: "Padrão claro", dark: false },
  { id: "dark", name: "Padrão escuro", dark: true },
  { id: "outlook", name: "Azul Outlook", dark: false },
  { id: "graphite", name: "Grafite", dark: true },
  { id: "midnight", name: "Meia-noite", dark: true },
  { id: "paper", name: "Sépia / Papel", dark: false },
  { id: "forest", name: "Floresta", dark: false },
  { id: "ocean", name: "Oceano", dark: true },
  { id: "lavender", name: "Lavanda", dark: false },
  { id: "rose", name: "Rosé", dark: false },
  { id: "amber", name: "Âmbar", dark: true },
  { id: "nord", name: "Nord", dark: true },
  { id: "solarized-light", name: "Solarized claro", dark: false },
  { id: "solarized-dark", name: "Solarized escuro", dark: true },
  { id: "dracula", name: "Dracula", dark: true },
  { id: "oled", name: "OLED preto", dark: true },
  { id: "hc-light", name: "Alto contraste claro", dark: false, hc: true },
  { id: "hc-dark", name: "Alto contraste escuro", dark: true, hc: true },
];

/** Atalhos de cor de destaque (hex, nome). Cada tema ajusta a cor ao seu contraste. */
export const ACCENTS: [string, string][] = [
  ["#0f6cbd", "azul"], ["#0e7c86", "turquesa"], ["#2e7d32", "verde"], ["#c2570c", "laranja"], ["#c62839", "vermelho"], ["#a23b8f", "magenta"], ["#6b4fbb", "roxo"],
];

const KEY = "posta.appearance";
const DARK_QUERY = "(prefers-color-scheme: dark)";

/** Valor salvo em `settings.theme`: "system" ou o id de um preset (valor desconhecido cai em "system"). */
export function themePref(v: string | undefined): string {
  return THEMES.some((t) => t.id === v) ? v! : "system";
}

export const resolveTheme = (pref: string) => (pref === "system" ? (window.matchMedia(DARK_QUERY).matches ? "dark" : "light") : pref);

/** Avisa quando o aparelho troca claro/escuro (só importa com "Seguir o sistema"). */
export function watchSystem(cb: () => void): () => void {
  const mq = window.matchMedia(DARK_QUERY);
  mq.addEventListener("change", cb);
  return () => mq.removeEventListener("change", cb);
}

// ---------- contraste ----------
type RGB = [number, number, number];
const toRgb = (hex: string): RGB => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as RGB;
const toHex = (c: RGB) => "#" + c.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");
const mixRgb = (a: RGB, b: RGB, t: number): RGB => [0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t) as RGB;
function lum(c: RGB) {
  const [r, g, b] = c.map((v) => { const x = v / 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a: RGB, b: RGB) {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

export interface Accent {
  blue: string;
  blueBg: string;
  onBlue: string;
  /** O destaque escolhido foi clareado/escurecido para manter o contraste. */
  adjusted: boolean;
}

/** Destaque próprio: aproxima a cor do texto do tema até ter contraste mínimo com as superfícies (4,5:1; 7:1 em alto contraste). */
export function fitAccent(hex: string, surface: string, selected: string, text: string, min: number): Accent {
  const base = toRgb(hex), s = toRgb(surface), sel = toRgb(selected), t = toRgb(text);
  let blue = base, step = 0;
  while ((contrast(blue, s) < min || contrast(blue, sel) < min) && step < 20) blue = mixRgb(base, t, ++step / 20);
  const tint = mixRgb(blue, s, 0.84);
  const onBlue: RGB = contrast([255, 255, 255], blue) >= contrast([11, 11, 11], blue) ? [255, 255, 255] : [11, 11, 11];
  return { blue: toHex(blue), blueBg: toHex(tint), onBlue: toHex(onBlue), adjusted: toHex(blue) !== hex.toLowerCase() };
}

/** Troca suave: durante ~250 ms as cores animam (styles.css, [data-theme-swap]). */
let swapTimer: number | undefined;
function swapFlash(root: HTMLElement) {
  root.setAttribute("data-theme-swap", "");
  window.clearTimeout(swapTimer);
  swapTimer = window.setTimeout(() => root.removeAttribute("data-theme-swap"), 250);
}

const token = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/** Aplica tema e destaque no <html> e atualiza o espelho. Devolve o destaque efetivo (ou null sem destaque próprio). */
export function applyAppearance(pref: string, accentHex: string | undefined): Accent | null {
  const root = document.documentElement;
  const id = resolveTheme(pref);
  if (root.dataset.theme && root.dataset.theme !== id) swapFlash(root);
  root.dataset.theme = id;
  for (const p of ["--blue", "--blue-bg", "--on-blue"]) root.style.removeProperty(p);
  let accent: Accent | null = null;
  if (accentHex && /^#[0-9a-f]{6}$/i.test(accentHex)) {
    const info = THEMES.find((t) => t.id === id);
    accent = fitAccent(accentHex, token("--surface"), token("--selected"), token("--text"), info?.hc ? 7 : 4.5);
    root.style.setProperty("--blue", accent.blue);
    root.style.setProperty("--blue-bg", accent.blueBg);
    root.style.setProperty("--on-blue", accent.onBlue);
  }
  try {
    localStorage.setItem(KEY, JSON.stringify({ theme: pref, accent: accent && { theme: id, blue: accent.blue, blueBg: accent.blueBg, onBlue: accent.onBlue } }));
  } catch { /* sem armazenamento: só perde o anti-flash */ }
  return accent;
}
