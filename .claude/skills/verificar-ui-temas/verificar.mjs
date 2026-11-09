// Verifica os temas no mock (Vite em PORT, padrão 36481): troca por clique, persistência, anti-flash, axe e prints.
// Variáveis: PLAYWRIGHT (pasta de playwright-core), AXE (axe.min.js), EXE (chrome-headless-shell), PORT, OUT (pasta dos PNG).
import fs from "node:fs";
import { createRequire } from "node:module";

const need = (k) => { if (!process.env[k]) { console.error(`Defina ${k}`); process.exit(2); } return process.env[k]; };
const { chromium } = createRequire(import.meta.url)(need("PLAYWRIGHT"));
const axeSrc = fs.readFileSync(need("AXE"), "utf8");
const URL = `http://localhost:${process.env.PORT ?? 36481}/`;
const OUT = process.env.OUT ?? "/home/renan/.t3/userdata/attachments";
const AXE_THEMES = ["light", "dark", "hc-light", "hc-dark", "nord", "paper"]; // claro, escuro, ambos de alto contraste, +2
const SHOTS = { light: "padrao", nord: "nord", paper: "papel", "hc-dark": "alto-contraste" };

const browser = await chromium.launch({ executablePath: process.env.EXE });
const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, colorScheme: "light" });
const page = await ctx.newPage();
const errors = [];
page.on("console", (m) => { if (["error", "warning"].includes(m.type())) errors.push(m.text()); });
page.on("pageerror", (e) => errors.push(String(e)));
let fail = 0;
const ok = (cond, msg) => { console.log(`${cond ? "ok  " : "FALHA"} ${msg}`); if (!cond) fail++; };
const theme = () => page.evaluate(() => document.documentElement.dataset.theme);
const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);

await page.goto(URL);
await page.getByRole("button", { name: "Configurações" }).click();
await page.getByRole("button", { name: "Aparência" }).click();
const cards = page.locator(".theme-card");
ok((await cards.count()) >= 17, `grade tem ${await cards.count()} cartões (>= 17: sistema + 16 presets)`);
ok((await theme()) === "light", "sem escolha, 'Seguir o sistema' resolve para claro");

// Troca instantânea por clique + miniatura
for (const id of ["dark", "nord", "paper", "hc-dark", "hc-light"]) {
  const before = await bg();
  await page.locator(`.theme-card:has(> .theme-thumb[data-theme="${id}"])`).click();
  ok((await theme()) === id, `clique aplica ${id}`);
  await page.waitForTimeout(300);
  if (id !== "dark") ok((await bg()) !== before, `fundo mudou em ${id}`);
}

// Persistência e anti-flash: recarrega com hc-dark, o atributo já existe no primeiro paint (script síncrono do <head>)
await page.locator('.theme-card:has([data-theme="hc-dark"])').click();
await page.waitForTimeout(500);
await page.addInitScript(() => {
  new MutationObserver(() => {}).observe(document, { childList: true });
  window.__first = null;
  const t = setInterval(() => {
    if (document.body && window.__first === null) { window.__first = document.documentElement.dataset.theme; clearInterval(t); }
  }, 0);
});
await page.reload({ waitUntil: "commit" });
await page.waitForSelector("body");
ok((await page.evaluate(() => window.__first)) === "hc-dark", "tema correto já quando o <body> aparece (sem flash)");
await page.getByRole("button", { name: "Configurações" }).click();
ok((await theme()) === "hc-dark", "persiste após recarregar");

// Destaque personalizado
await page.getByRole("button", { name: "Aparência" }).click();
await page.getByRole("button", { name: "Destaque laranja" }).click();
const blue = await page.evaluate(() => document.documentElement.style.getPropertyValue("--blue"));
ok(/^#[0-9a-f]{6}$/.test(blue), `destaque próprio aplicado (${blue})`);
await page.getByRole("button", { name: "Do tema" }).click();
ok((await page.evaluate(() => document.documentElement.style.getPropertyValue("--blue"))) === "", "'Do tema' remove o destaque");

// Seguir o sistema acompanha o SO
await page.locator(".theme-card", { hasText: "Seguir o sistema" }).click();
await page.emulateMedia({ colorScheme: "dark" });
await page.waitForTimeout(300);
ok((await theme()) === "dark", "seguir o sistema acompanha o escuro do SO");
await page.emulateMedia({ colorScheme: "light" });

// axe + prints
fs.mkdirSync(OUT, { recursive: true });
await page.locator(".theme-card", { hasText: "Padrão claro" }).click();
await page.waitForTimeout(300);
await page.locator(".toast").first().waitFor({ state: "detached", timeout: 8000 }).catch(() => {});
await page.setViewportSize({ width: 1280, height: 1400 });
await page.screenshot({ path: `${OUT}/posta-temas-aparencia.png` });
await page.setViewportSize({ width: 1280, height: 800 });
for (const id of AXE_THEMES) {
  await page.getByRole("button", { name: "Voltar" }).click().catch(() => {});
  await page.evaluate((t) => { document.documentElement.dataset.theme = t; }, id); // mesmo efeito do clique, para ver o app
  await page.waitForTimeout(450);
  await page.locator(".toast").first().waitFor({ state: "detached", timeout: 8000 }).catch(() => {}); // toast saindo faz o axe medir cor em transição
  await page.addScriptTag({ content: axeSrc });
  const res = await page.evaluate(() => window.axe.run(document, { resultTypes: ["violations"] }));
  const bad = res.violations.filter((v) => ["critical", "serious"].includes(v.impact));
  ok(bad.length === 0, `axe no app (${id}): ${bad.map((v) => v.id + "×" + v.nodes.length + " " + v.nodes.slice(0, 2).map((n) => n.target + ": " + (n.any[0]?.message ?? "")).join("; ")).join(", ") || "sem Critical/Serious"}`);
  if (SHOTS[id]) await page.screenshot({ path: `${OUT}/posta-temas-app-${SHOTS[id]}.png` });
  await page.getByRole("button", { name: "Configurações" }).click();
  await page.getByRole("button", { name: "Aparência" }).click();
  await page.locator(`.theme-card:has(> .theme-thumb[data-theme="${id}"])`).click();
  await page.waitForTimeout(450);
  await page.locator(".toast").first().waitFor({ state: "detached", timeout: 8000 }).catch(() => {});
  const res2 = await page.evaluate(() => window.axe.run(document, { resultTypes: ["violations"] }));
  const bad2 = res2.violations.filter((v) => ["critical", "serious"].includes(v.impact));
  ok(bad2.length === 0, `axe em Aparência (${id}): ${bad2.map((v) => v.id + "×" + v.nodes.length + " " + v.nodes.slice(0, 2).map((n) => n.target + ": " + (n.any[0]?.message ?? "")).join("; ")).join(", ") || "sem Critical/Serious"}`);
}
ok(errors.length === 0, `console sem erro nem aviso ${errors.slice(0, 3).join(" | ")}`);
await browser.close();
process.exit(fail ? 1 : 0);
