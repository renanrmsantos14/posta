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

// Preview, estilo (cantos, sombras, fonte, barra lateral) e tema personalizado
const root = (a) => page.evaluate((n) => document.documentElement.getAttribute(n), a);
const pv = (a) => page.locator(".pv").getAttribute(a);
const seg = (label, text) => page.getByRole("radiogroup", { name: label }).getByRole("radio", { name: text }).click();
ok((await page.locator('.pv[inert][aria-hidden="true"]').count()) === 1, "preview decorativo (inert + aria-hidden)");
await page.locator(".theme-card", { hasText: "Padrão claro" }).click();
await seg("Cantos", "Retos");
ok((await root("data-radius")) === "sharp" && (await pv("data-radius")) === "sharp", "Cantos retos no app e no preview");
ok((await page.evaluate(() => getComputedStyle(document.querySelector(".pv .nav-item")).borderRadius)) === "0px", "raio do item do preview vai a 0");
await seg("Cantos", "Arredondados");
ok((await page.evaluate(() => getComputedStyle(document.querySelector(".pv .nav-item")).borderRadius)) === "9.6px", "Arredondados: 6px x 1,6");
await seg("Cantos", "Médios");
ok((await page.evaluate(() => getComputedStyle(document.querySelector(".pv .nav-item")).borderRadius)) === "6px", "Médios = visual original (6px)");
await seg("Sombras", "Nenhuma");
ok((await root("data-shadow")) === "none" && (await pv("data-shadow")) === "none", "Sombras nenhuma");
await seg("Sombras", "Padrão");
await seg("Fonte da interface", "Sistema");
ok((await page.evaluate(() => getComputedStyle(document.querySelector(".pv")).fontFamily)).startsWith("system-ui"), "fonte Sistema no preview");
await seg("Fonte da interface", "Geist");
const rowH = () => page.evaluate(() => document.querySelector(".pv .nav-item").getBoundingClientRect().height);
const med = await rowH();
await seg("Densidade das linhas da barra lateral", "Compacta");
ok((await rowH()) < med && (await root("data-rail")) === "compact", "densidade compacta da barra encolhe os itens");
await seg("Densidade das linhas da barra lateral", "Confortável");
ok((await rowH()) > med, "densidade confortável da barra aumenta os itens");
await seg("Densidade das linhas da barra lateral", "Média");
const open = () => page.locator(".pv .acct-sec[data-open]").count();
await seg("Contas abertas na barra lateral", "Uma por vez");
ok((await open()) === 1, "Uma por vez: só uma conta aberta no preview");
await seg("Contas abertas na barra lateral", "Várias abertas");
ok((await open()) === 2, "Várias abertas: as duas contas abertas");

const cardsBefore = await page.locator(".theme-card").count();
await page.getByRole("button", { name: "Personalizar" }).click();
await page.getByLabel("Nome do tema").fill("Teste verde");
await page.locator('input[data-token="--bg"]').fill("#e8f5e9");
ok((await page.evaluate(() => getComputedStyle(document.querySelector(".pv")).getPropertyValue("--bg").trim())) === "#e8f5e9", "preview reflete a cor editada antes de salvar");
await page.locator('input[data-token="--text"]').fill("#cccccc");
ok((await page.locator(".theme-warn").count()) > 0, "aviso de contraste com texto claro sobre superfície clara");
await page.locator('input[data-token="--text"]').fill("#1a2b1c");
await page.getByRole("button", { name: "Salvar tema" }).click();
ok((await page.locator(".theme-card").count()) === cardsBefore + 1, "tema personalizado entra na grade");
ok((await page.evaluate(() => document.documentElement.style.getPropertyValue("--bg"))) === "#e8f5e9", "tema personalizado aplicado no <html>");
await page.reload({ waitUntil: "commit" });
await page.waitForSelector("body");
ok((await page.evaluate(() => document.documentElement.style.getPropertyValue("--bg"))) === "#e8f5e9", "tema personalizado já aplicado no primeiro paint (espelho)");
await page.getByRole("button", { name: "Configurações" }).click();
await page.getByRole("button", { name: "Aparência" }).click();
await page.getByRole("button", { name: "Renomear Teste verde" }).click();
await page.getByLabel("Novo nome de Teste verde").fill("Verde 2");
await page.keyboard.press("Enter");
ok((await page.locator(".theme-card", { hasText: "Verde 2" }).count()) === 1, "renomear atualiza o cartão");
await page.getByRole("button", { name: "Excluir Verde 2" }).click();
ok((await page.locator(".theme-card", { hasText: "Verde 2" }).count()) === 1, "excluir pede confirmação antes de apagar");
await page.getByRole("button", { name: "Excluir", exact: true }).click();
ok((await page.locator(".theme-card").count()) === cardsBefore && (await page.evaluate(() => document.documentElement.style.getPropertyValue("--bg"))) === "", "excluir remove o tema e as variáveis");

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
