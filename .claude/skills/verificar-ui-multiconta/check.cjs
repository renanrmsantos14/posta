// Uso: node check.cjs <url> <saida-dos-prints> [caminho-do-playwright-core]
// Confere as regras do spec docs/superpowers/specs/2026-11-10-navegacao-multiconta-design.md no mock.
const pwPath = process.argv[4] || 'playwright-core';
const { chromium } = require(pwPath);
const url = process.argv[2] || 'http://localhost:36472';
const out = process.argv[3] || '.';
const fails = [];
const ok = (cond, msg) => { console.log(`${cond ? '✓' : '✗'} ${msg}`); if (!cond) fails.push(msg); };
(async () => {
  const exe = process.env.CHROME || `${process.env.HOME}/.cache/ms-playwright/chromium-1148/chrome-linux/chrome`;
  const b = await chromium.launch({ executablePath: exe });
  const errs = [];
  const page = async (vp, q = '?contas=5') => {
    const p = await b.newPage({ viewport: vp, colorScheme: 'dark' });
    p.on('console', (m) => { if (['error', 'warning'].includes(m.type())) errs.push(m.text()); });
    p.on('pageerror', (e) => errs.push(e.message));
    await p.goto(`${url}/${q}`); await p.waitForTimeout(1200);
    return p;
  };

  // Desktop, 5 contas.
  let p = await page({ width: 1280, height: 800 });
  const heads = p.locator('.rail .acct-head');
  ok(await heads.count() === 5, 'uma seção por conta (5)');
  ok(await p.locator('.rail .acct-strip button').count() === 6, 'faixa: Todas + 5 contas');
  const sec = (i) => p.locator('.rail .acct-sec').nth(i);
  // Contador da seção = não lidas da Entrada da conta; Todas › Entrada = soma.
  const nums = [];
  for (let i = 0; i < 5; i++) nums.push(Number(await sec(i).locator('.acct-head .count').textContent().catch(() => '0')) || 0);
  const all = Number(await p.locator('.rail .rail-section[aria-label="Todas as contas"] .nav-item .count').first().textContent());
  ok(nums.reduce((a, n) => a + n, 0) === all, `Todas › Entrada (${all}) soma as seções (${nums.join('+')})`);
  // Acordeão: abrir a 2ª fecha as outras; nenhuma pasta de outra conta aparece.
  await heads.nth(0).click(); await p.waitForTimeout(300);
  await heads.nth(1).click(); await p.waitForTimeout(300);
  ok(await p.locator('.rail .acct-sec[data-open]').count() === 1, 'só uma seção aberta por vez');
  await sec(1).locator('.nav-item:visible').first().click(); await p.waitForTimeout(400);
  const scope = (await p.locator('.list-scope').textContent()).replace('›', '').trim();
  const name = (await sec(1).locator('.acct-name').textContent()).trim();
  ok(scope === name, `cabeçalho da lista mostra a conta (${scope})`);
  ok(Number(await p.locator('.tab .count, [role=tab] .count').first().textContent().catch(() => '0')) <= nums[1], 'Foco da conta não passa do contador da seção');
  await p.screenshot({ path: `${out}/multiconta-desktop.png` });
  await p.close();

  // Uma conta só: sem faixa nem "Todas as contas".
  p = await page({ width: 1280, height: 800 }, '?contas=1');
  ok(await p.locator('.rail .acct-strip, .rail .acct-sec').count() === 0, 'com 1 conta não há faixa nem seções (Rail como antes)');
  ok(await p.locator('.rail .rail-label', { hasText: 'Todas as contas' }).count() === 0, 'com 1 conta não aparece "Todas as contas"');
  await p.screenshot({ path: `${out}/multiconta-uma-conta.png` });
  await p.close();

  // Celular: gaveta abre, troca de conta, fecha ao escolher, ao tocar fora, no X e no Voltar.
  p = await page({ width: 390, height: 844 });
  const menu = p.getByRole('button', { name: 'Pastas e contas' });
  await menu.click(); await p.waitForTimeout(400);
  ok(await p.locator('.drawer-panel').isVisible(), 'gaveta abre pelo ≡');
  await p.screenshot({ path: `${out}/multiconta-gaveta.png` });
  await p.locator('.drawer-panel .acct-head').nth(2).click(); await p.waitForTimeout(300);
  await p.locator('.drawer-panel .acct-sec[data-open] .nav-item').first().click(); await p.waitForTimeout(500);
  ok(!(await p.locator('.drawer-panel').isVisible().catch(() => false)), 'gaveta fecha ao escolher pasta');
  ok((await p.locator('.list-scope').textContent()).includes((await p.locator('.rail .acct-name').nth(2).textContent().catch(() => '')) || ''), 'título reflete a conta escolhida');
  await menu.click(); await p.waitForTimeout(400);
  await p.mouse.click(370, 400); await p.waitForTimeout(400);
  ok(!(await p.locator('.drawer-panel').isVisible().catch(() => false)), 'toque fora fecha');
  await menu.click(); await p.waitForTimeout(400);
  await p.getByRole('button', { name: 'Fechar' }).click(); await p.waitForTimeout(400);
  ok(!(await p.locator('.drawer-panel').isVisible().catch(() => false)), 'X fecha');
  await menu.click(); await p.waitForTimeout(400);
  await p.goBack(); await p.waitForTimeout(400);
  ok(!(await p.locator('.drawer-panel').isVisible().catch(() => false)), 'Voltar fecha');
  await p.close();

  ok(errs.length === 0, `console sem erro/aviso${errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''}`);
  await b.close();
  process.exit(fails.length ? 1 : 0);
})();
