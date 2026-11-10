// Verifica no mock (Vite em PORT, padrão 36480) o ajuste e o zoom do corpo HTML no leitor e as preferências da barra de ferramentas.
import { createRequire } from "node:module";
const need = (k) => { if (!process.env[k]) { console.error(`Defina ${k}`); process.exit(2); } return process.env[k]; };
const { chromium } = createRequire(import.meta.url)(need("PLAYWRIGHT"));
const URL = `http://localhost:${process.env.PORT ?? 36480}/`;
const OUT = process.env.OUT ?? "/home/renan/.t3/userdata/attachments";
let fails = 0;
const check = (ok, msg) => { console.log(`${ok ? "ok  " : "FALHA"} ${msg}`); if (!ok) fails++; };

const browser = await chromium.launch({ executablePath: process.env.EXE });
const page = await browser.newPage({ viewport: { width: 1920, height: 1000 } });
const logs = [];
page.on("console", (m) => { if (["error", "warning"].includes(m.type()) && !m.text().includes("about:srcdoc")) logs.push(m.text()); });
await page.goto(URL); await page.evaluate(() => localStorage.clear()); await page.reload(); await page.waitForTimeout(1500);

const openSample = async () => {
  await page.getByRole("tab", { name: /Outros/ }).click();
  await page.getByText("Novidades da semana: rotas").first().click();
  await page.waitForTimeout(700);
};
const frame = () => page.evaluate(() => {
  const f = document.querySelector("iframe.html-body"); const d = f.contentDocument.documentElement;
  return { w: f.clientWidth, zoom: Number(d.style.zoom), ox: d.style.overflowX, over: d.scrollWidth > d.clientWidth + 1 };
});

await openSample();
let s = await frame();
check(s.zoom === 1 && !s.over, `painel largo: zoom 1, sem rolagem lateral (${JSON.stringify(s)})`);
await page.setViewportSize({ width: 1100, height: 900 }); await page.waitForTimeout(600);
s = await frame();
check(s.zoom < 1 && !s.over && s.ox === "hidden", `painel estreito: tabela de 680 px encolhe para caber (${JSON.stringify(s)})`);
await page.screenshot({ path: `${OUT}/posta-leitor-ajuste.png` });
const fit = s.zoom;
await page.getByRole("button", { name: "Aumentar zoom" }).click(); await page.getByRole("button", { name: "Aumentar zoom" }).click(); await page.waitForTimeout(400);
s = await frame();
const pct = await page.locator(".zoom-pct").textContent();
check(pct === "125%" && Math.abs(s.zoom - fit * 1.25) < 0.01 && s.ox === "auto", `+ + → 125%, zoom = ajuste × 1,25 com rolagem lateral (${pct}, ${JSON.stringify(s)})`);
await page.reload(); await page.waitForTimeout(1200); await openSample();
check(await page.locator(".zoom-pct").textContent() === "125%", "zoom persiste após recarregar (posta.readerZoom)");
await page.locator("iframe.html-body").click({ position: { x: 20, y: 20 } });
await page.keyboard.press("Control+0"); await page.waitForTimeout(300);
check(await page.locator(".zoom-pct").textContent() === "100%", "Ctrl+0 dentro do e-mail volta a 100%");
await page.mouse.move(500, 500);
const box = await page.locator("iframe.html-body").boundingBox();
await page.mouse.move(box.x + 50, box.y + 50); await page.keyboard.down("Control"); await page.mouse.wheel(0, -100); await page.keyboard.up("Control"); await page.waitForTimeout(300);
check(await page.locator(".zoom-pct").textContent() === "110%", "Ctrl+roda para cima no e-mail aumenta um passo");
await page.locator(".zoom-pct").click(); await page.waitForTimeout(200);

// Barra de ferramentas
await page.setViewportSize({ width: 1440, height: 900 });
const bar = () => page.evaluate(() => {
  const h = document.querySelector(".cmdbar-host"); const c = h.querySelector(".cmd");
  const labels = [...h.querySelectorAll(".cmd:not([data-primary]) .cmd-label, .cmd:not([data-primary]) span")].filter((e) => e.getClientRects().length).length;
  return { size: h.dataset.size, labels: h.dataset.labels, h: Math.round(c.getBoundingClientRect().height), visibleLabels: labels, ids: [...h.querySelectorAll(".cmd")].map((b) => b.getAttribute("aria-label")) };
});
const before = await bar();
check(before.size === "m" && before.h === 32, `padrão Média: botões de 32 px (${JSON.stringify({ size: before.size, h: before.h })})`);
await page.getByRole("button", { name: /Configura/ }).first().click(); await page.waitForTimeout(400);
await page.getByText("Aparência").first().click(); await page.waitForTimeout(300);
await page.getByRole("radiogroup", { name: "Tamanho da barra de ferramentas" }).getByRole("radio", { name: "Grande" }).click();
await page.getByRole("radiogroup", { name: "Rótulos da barra" }).getByRole("radio", { name: "Só ícones" }).click();
await page.locator(".tool-pick", { hasText: "Encaminhar" }).locator("input").uncheck(); await page.waitForTimeout(300);
await page.locator(".tool-picks").scrollIntoViewIfNeeded();
await page.screenshot({ path: `${OUT}/posta-barra-config.png` });
await page.keyboard.press("Escape"); await page.waitForTimeout(300);
let after = await bar();
check(after.size === "l" && after.h === 40, `Grande: botões de 40 px (${after.h})`);
check(after.labels === "icons" && after.visibleLabels === 0, `Só ícones: nenhum rótulo visível além do primário (${after.visibleLabels})`);
check(!after.ids.includes("Encaminhar"), "Encaminhar desmarcado some da barra");
await page.reload(); await page.waitForTimeout(1200);
after = await bar();
check(after.size === "l" && after.labels === "icons" && !after.ids.includes("Encaminhar"), "preferências da barra persistem após recarregar");
await page.screenshot({ path: `${OUT}/posta-barra-grande.png` });

// Sempre com rótulos numa janela estreita: todo botão alcançável pela faixa ou pelo menu "…"
await page.getByRole("button", { name: /Configura/ }).first().click(); await page.waitForTimeout(400);
await page.getByText("Aparência").first().click(); await page.waitForTimeout(300);
await page.getByRole("radiogroup", { name: "Rótulos da barra" }).getByRole("radio", { name: "Sempre" }).click();
await page.keyboard.press("Escape"); await page.setViewportSize({ width: 900, height: 800 }); await page.waitForTimeout(400);
const scroll = await page.evaluate(() => { const b = document.querySelector(".cmdbar"); return { sw: b.scrollWidth, cw: b.clientWidth, ox: getComputedStyle(b).overflowX }; });
check(scroll.ox === "auto", `Sempre: faixa rola na horizontal quando falta espaço (${JSON.stringify(scroll)})`);
check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "sem rolagem horizontal da página a 900 px");

check(logs.length === 0, `console sem erro nem aviso${logs.length ? ": " + logs.slice(0, 3).join(" | ") : ""}`);
await browser.close();
console.log(fails ? `${fails} falha(s)` : "tudo ok");
process.exit(fails ? 1 : 0);
