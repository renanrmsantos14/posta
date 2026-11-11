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

/** Atualiza o espelho em localStorage (lido por public/theme-init.js); mescla com o que já existe. */
function mirror(patch: Record<string, unknown>) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...JSON.parse(localStorage.getItem(KEY) || "{}"), ...patch }));
  } catch { /* sem armazenamento: só perde o anti-flash */ }
}

// ---------- tema personalizado ----------
/** Tokens editáveis: chave, variável CSS e rótulo. O resto (semânticas, hover, color-scheme) vem do tema base. */
export const TOKENS = [
  ["bg", "--bg", "Fundo"], ["surface", "--surface", "Superfície"], ["rail", "--rail", "Barra lateral"],
  ["text", "--text", "Texto"], ["strong", "--strong", "Texto forte"], ["muted", "--muted", "Texto secundário"],
  ["line", "--line", "Linhas"], ["selected", "--selected", "Seleção"], ["blue", "--blue", "Destaque"],
] as const;
export type TokenKey = (typeof TOKENS)[number][0];
export type Colors = Record<TokenKey, string>;

export interface CustomTheme {
  id: string;
  name: string;
  /** Tema embutido de onde herda o restante (claro/escuro, cores semânticas). */
  base: string;
  colors: Colors;
}

export const HEX = /^#[0-9a-f]{6}$/i;

/** Lê `settings.customThemes` (JSON); descarta o que não for válido. */
export function parseCustomThemes(raw: string | undefined): CustomTheme[] {
  try {
    const list: unknown = JSON.parse(raw || "[]");
    if (!Array.isArray(list)) return [];
    return list.filter((c): c is CustomTheme =>
      typeof c?.id === "string" && typeof c.name === "string" && THEMES.some((t) => t.id === c.base)
      && TOKENS.every(([k]) => HEX.test(c.colors?.[k] ?? "")));
  } catch { return []; }
}

export const newCustomId = () => `c-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 5)}`;

/** Cores atuais de um tema embutido, lidas dos tokens do CSS (themes.css). */
export function readColors(base: string): Colors {
  const el = document.createElement("div");
  el.dataset.theme = base;
  document.body.appendChild(el);
  const cs = getComputedStyle(el);
  const out = {} as Colors;
  for (const [key, css] of TOKENS) {
    const v = cs.getPropertyValue(css).trim();
    out[key] = HEX.test(v) ? v.toLowerCase() : "#000000";
  }
  el.remove();
  return out;
}

/** Variáveis CSS de um tema personalizado: as editadas mais as derivadas (destaque ajustado, tom claro, texto sobre destaque, botão). */
export function customVars(c: Pick<CustomTheme, "colors" | "base">): Record<string, string> {
  const k = c.colors;
  const accent = fitAccent(k.blue, k.surface, k.selected, k.text, minContrast(c.base));
  const vars: Record<string, string> = {};
  for (const [key, css] of TOKENS) vars[css] = k[key];
  return {
    ...vars, "--blue": accent.blue, "--blue-bg": accent.blueBg, "--on-blue": accent.onBlue,
    "--faint": toHex(mixRgb(toRgb(k.muted), toRgb(k.bg), 0.12)),
    "--btn": k.strong, "--btn-text": k.bg, "--btn-hover": `color-mix(in srgb, ${k.strong} 80%, ${k.bg})`,
  };
}

/** Tema base (`data-theme`) e variáveis do tema escolhido, para o <html> e para as miniaturas/preview. */
export function themeStyle(pref: string, customs: CustomTheme[]): { id: string; vars: Record<string, string> } {
  const custom = customs.find((c) => c.id === pref);
  return custom ? { id: custom.base, vars: customVars(custom) } : { id: resolveTheme(pref), vars: {} };
}

/** Contraste mínimo do destaque: AAA (7:1) nos temas de alto contraste (e nos personalizados feitos a partir deles), AA (4,5:1) nos demais. */
export const minContrast = (base: string) => (THEMES.find((t) => t.id === base)?.hc ? 7 : 4.5);

/** Destaque próprio sobre as cores efetivas do tema (preview e <html> usam a mesma regra). */
export function accentOver(id: string, colors: Colors, accentHex: string | undefined): Accent | null {
  if (!accentHex || !HEX.test(accentHex)) return null;
  return fitAccent(accentHex, colors.surface, colors.selected, colors.text, minContrast(id));
}

// ---------- estilo (cantos, sombras, fonte, barra lateral) ----------
/** Chave da configuração -> atributo no <html> (styles.css), valores aceitos e padrão. */
export const STYLE = {
  radius: { attr: "data-radius", values: ["sharp", "soft", "medium", "round"], def: "medium" },
  shadow: { attr: "data-shadow", values: ["none", "soft", "default", "strong"], def: "default" },
  font: { attr: "data-font", values: ["geist", "system", "inter", "serif"], def: "geist" },
  railDensity: { attr: "data-rail", values: ["compact", "medium", "comfortable"], def: "medium" },
} as const;
export type StyleKey = keyof typeof STYLE;
export const styleValue = (key: StyleKey, v: string | undefined): string =>
  (STYLE[key].values as readonly string[]).includes(v ?? "") ? v! : STYLE[key].def;

/** Aplica cantos, sombras, fonte e densidade da barra no <html> e guarda no espelho (anti-flash). */
export function applyStyle(settings: Partial<Record<StyleKey, string>>) {
  const root = document.documentElement;
  const style = {} as Record<StyleKey, string>;
  for (const key of Object.keys(STYLE) as StyleKey[]) {
    style[key] = styleValue(key, settings[key]);
    root.setAttribute(STYLE[key].attr, style[key]);
  }
  mirror({ style });
}

/** Valor salvo em `settings.theme`: "system", o id de um preset ou de um tema personalizado (desconhecido cai em "system"). */
export function themePref(v: string | undefined, customs: CustomTheme[] = []): string {
  return THEMES.some((t) => t.id === v) || customs.some((c) => c.id === v) ? v! : "system";
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
/** Contraste WCAG entre duas cores `#rrggbb`. */
export const contrastRatio = (a: string, b: string) => contrast(toRgb(a), toRgb(b));

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

/** Variáveis que o tema personalizado e o destaque próprio põem no <html>; saem antes de cada aplicação. */
const OWNED_VARS = [...TOKENS.map(([, css]) => css), "--blue-bg", "--on-blue", "--faint", "--btn", "--btn-text", "--btn-hover"];
let lastApplied: string | undefined;

/** Aplica tema (embutido ou personalizado) e destaque no <html> e atualiza o espelho. Devolve o destaque efetivo (ou null sem destaque próprio). */
export function applyAppearance(pref: string, accentHex: string | undefined, customs: CustomTheme[] = []): Accent | null {
  const root = document.documentElement;
  const { id, vars } = themeStyle(pref, customs);
  const applied = vars["--bg"] ? pref : id;
  if (lastApplied !== undefined && lastApplied !== applied) swapFlash(root);
  lastApplied = applied;
  root.dataset.theme = id;
  for (const p of OWNED_VARS) root.style.removeProperty(p);
  for (const [p, v] of Object.entries(vars)) root.style.setProperty(p, v);
  const cs = getComputedStyle(root);
  const colors = Object.fromEntries(TOKENS.map(([k, css]) => [k, cs.getPropertyValue(css).trim()])) as Colors;
  const accent = accentOver(id, colors, accentHex);
  if (accent) {
    root.style.setProperty("--blue", accent.blue);
    root.style.setProperty("--blue-bg", accent.blueBg);
    root.style.setProperty("--on-blue", accent.onBlue);
  }
  mirror({
    theme: pref,
    accent: accent && { theme: id, blue: accent.blue, blueBg: accent.blueBg, onBlue: accent.onBlue },
    custom: vars["--bg"] ? { theme: pref, base: id, vars } : null,
  });
  return accent;
}
