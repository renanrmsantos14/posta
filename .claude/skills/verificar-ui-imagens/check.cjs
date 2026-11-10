// Uso: node check.cjs <url> <saida-dos-prints> [caminho-do-playwright-core]
// Confere no mock o bloqueio de imagens remotas e a liberação por remetente (setting `images:trusted`).
const { chromium } = require(process.argv[4] || 'playwright-core');
const url = process.argv[2] || 'http://localhost:36472';
const out = process.argv[3] || '.';
const fails = [];
const ok = (cond, msg) => { console.log(`${cond ? '✓' : '✗'} ${msg}`); if (!cond) fails.push(msg); };
(async () => {
  const b = await chromium.launch(process.env.CHROME ? { executablePath: process.env.CHROME } : {});
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  // A CSP recusa a imagem enquanto bloqueada e o placehold.co pode responder 403: esperado.
  const errs = []; p.on('console', (m) => ['error', 'warning'].includes(m.type()) && !/Content Security Policy|403/.test(m.text()) && errs.push(m.text()));
  await p.goto(url); await p.waitForTimeout(1000);
  await p.keyboard.press('Control+e'); await p.keyboard.type('casacos'); await p.keyboard.press('Enter'); await p.waitForTimeout(800);
  await p.locator('.list-spacer > .row').first().click(); await p.waitForTimeout(800);
  const banner = p.locator('.img-banner');
  const trust = p.getByRole('button', { name: 'Sempre mostrar de ofertas@lojanorte.com.br' });
  ok(await banner.count() === 1 && await trust.count() === 1, 'faixa com "Mostrar imagens" e "Sempre mostrar de ofertas@lojanorte.com.br"');
  await p.screenshot({ path: `${out}/imagens-faixa.png` });
  await trust.click(); await p.waitForTimeout(600);
  ok(await banner.count() === 0, 'liberar o remetente some com a faixa');
  await p.getByRole('button', { name: 'Desfazer' }).click(); await p.waitForTimeout(500);
  ok(await banner.count() === 1, 'Desfazer volta a bloquear');
  await trust.click(); await p.waitForTimeout(300);
  await p.getByRole('button', { name: 'Configurações' }).click(); await p.waitForTimeout(600);
  await p.getByText('Leitura e layout').first().click(); await p.waitForTimeout(500);
  const ta = p.getByLabel('Remetentes com imagens sempre liberadas');
  ok((await ta.inputValue()) === 'ofertas@lojanorte.com.br', 'Configurações › Leitura lista o remetente liberado');
  await p.screenshot({ path: `${out}/imagens-config.png` });
  ok(errs.length === 0, `console sem erros (${errs.length}) ${errs.slice(0, 2).join(' | ')}`);
  await b.close();
  process.exit(fails.length ? 1 : 0);
})();
