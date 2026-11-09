// Confere o contraste WCAG dos presets em apps/client/src/themes.css.
// Uso: node .claude/skills/verificar-ui-temas/contraste.mjs [caminho/themes.css]
// AA (4,5:1) em todos os pares de texto; presets "hc-*" exigem AAA (7:1). Sai com código 1 se algo falhar.
import fs from "node:fs";

const file = process.argv[2] ?? new URL("../../../apps/client/src/themes.css", import.meta.url).pathname;
const css = fs.readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

// Junta, em ordem, os blocos cujo seletor menciona o preset.
const blocks = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, sel, body]) => ({ sel, body }));
const vars = (id) => {
  const out = {};
  for (const { sel, body } of blocks) {
    if (!sel.includes(`[data-theme="${id}"]`) && !(id === "light" && sel.includes(":root"))) continue;
    for (const [, k, v] of body.matchAll(/--([\w-]+):\s*([^;]+);/g)) out[k] = v.trim();
  }
  return out;
};
const ids = [...new Set([...css.matchAll(/\[data-theme="([\w-]+)"\]/g)].map((m) => m[1]))];

const lum = (h) => {
  const c = [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};
const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const over = (rgba, base) => { // rgba(r, g, b, a) sobre um hex
  const m = rgba.match(/[\d.]+/g).map(Number);
  return "#" + [0, 1, 2].map((i) => Math.round(m[i] * m[3] + parseInt(base.slice(1 + 2 * i, 3 + 2 * i), 16) * (1 - m[3])).toString(16).padStart(2, "0")).join("");
};
const hex = (v, v0) => (v.startsWith("var(") ? v0[v.slice(6, -1)] : v);

let bad = 0;
for (const id of ids) {
  const raw = vars(id);
  const v = { ...vars("light"), ...raw };
  // Tokens derivados da família escura (var(--strong), var(--bg)).
  for (const k of Object.keys(v)) if (v[k].startsWith("var(")) v[k] = hex(v[k], v);
  if (v["btn-hover"]?.startsWith("color-mix")) v["btn-hover"] = v.btn;
  const min = id.startsWith("hc-") ? 7 : 4.5;
  const fails = [];
  const check = (name, a, b) => { const r = ratio(a, b); if (r < min) fails.push(`${name} ${r.toFixed(2)}`); };
  const hover = over(v.hover, v.surface);
  for (const bg of ["bg", "surface", "rail", "selected"]) for (const fg of ["text", "strong", "muted", "faint", "blue"]) check(`${fg}/${bg}`, v[fg], v[bg]);
  for (const fg of ["text", "strong", "muted", "faint"]) check(`${fg}/hover`, v[fg], hover);
  for (const k of ["green", "red", "yellow", "purple", "orange"]) { check(`${k}/${k}-bg`, v[k], v[`${k}-bg`]); check(`${k}/surface`, v[k], v.surface); }
  check("blue/blue-bg", v.blue, v["blue-bg"]);
  check("on-blue/blue", v["on-blue"], v.blue);
  check("btn-text/btn", v["btn-text"], v.btn);
  check("btn-text/btn-hover", v["btn-text"], v["btn-hover"]);
  console.log(`${id.padEnd(16)} ${min === 7 ? "AAA" : "AA "} ${fails.length ? "FALHA: " + fails.join(" | ") : "ok"}`);
  bad += fails.length;
}
console.log(`${ids.length} presets, ${bad} falhas`);
process.exit(bad ? 1 : 0);
