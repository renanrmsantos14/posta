// Rodar: pnpm --filter client test (Node 22.18+ lê .ts direto).
import test from "node:test";
import assert from "node:assert/strict";
import { fromLocalInput, isPast, partsOf, toLocalInput, whenPresets, zonedToUnix } from "./when.ts";

const at = (iso) => Date.parse(iso) / 1000; // iso com offset explícito
const keys = (now) => whenPresets(now).map((p) => p.key);
const find = (now, key) => whenPresets(now).find((p) => p.key === key)?.at;

test("fuso: Brasília é UTC-3 hoje (sem horário de verão)", () => {
  assert.equal(zonedToUnix(2026, 11, 27, 8, 0), at("2026-11-27T08:00:00-03:00"));
  assert.deepEqual(partsOf(at("2026-11-27T23:59:00-03:00")), { y: 2026, m: 11, d: 27, h: 23, mi: 59, wd: 5 });
});

test("hoje à noite: 20h, e só aparece antes das 20h", () => {
  assert.equal(find(at("2026-11-27T09:00:00-03:00"), "tonight"), at("2026-11-27T20:00:00-03:00"));
  assert.equal(find(at("2026-11-27T19:59:00-03:00"), "tonight"), at("2026-11-27T20:00:00-03:00"));
  assert.deepEqual(keys(at("2026-11-27T20:00:00-03:00")), ["tomorrow", "monday"]);
  assert.deepEqual(keys(at("2026-11-27T23:30:00-03:00")), ["tomorrow", "monday"]);
});

test("amanhã 8h e próxima segunda 8h", () => {
  const sex = at("2026-11-27T15:00:00-03:00"); // sexta
  assert.equal(find(sex, "tomorrow"), at("2026-11-28T08:00:00-03:00"));
  assert.equal(find(sex, "monday"), at("2026-11-30T08:00:00-03:00"));
  const dom = at("2026-11-29T10:00:00-03:00"); // domingo → segunda é amanhã
  assert.equal(find(dom, "monday"), at("2026-11-30T08:00:00-03:00"));
  const seg = at("2026-11-30T07:00:00-03:00"); // segunda: "próxima" é a da semana seguinte
  assert.equal(find(seg, "monday"), at("2026-12-07T08:00:00-03:00"));
});

test("virada de dia: 23h59 de Brasília já é o dia seguinte em UTC, mas 'amanhã' é o dia de Brasília", () => {
  const now = at("2026-12-31T23:59:00-03:00"); // 02:59Z de 01/01
  assert.equal(find(now, "tomorrow"), at("2027-01-01T08:00:00-03:00"));
  assert.equal(find(at("2026-02-28T22:00:00-03:00"), "tomorrow"), at("2026-03-01T08:00:00-03:00"));
  assert.equal(find(at("2028-02-28T22:00:00-03:00"), "tomorrow"), at("2028-02-29T08:00:00-03:00")); // bissexto
});

test("horário de verão antigo (2018): início 04/11 00:00 pula para 01:00", () => {
  // Fuso -3 até 03:00Z, depois -2.
  assert.equal(zonedToUnix(2018, 11, 3, 20, 0), at("2018-11-03T20:00:00-03:00"));
  assert.equal(zonedToUnix(2018, 11, 5, 8, 0), at("2018-11-05T08:00:00-02:00"));
  // 00:30 de 04/11 não existiu: vai para 01:30 (-02:00).
  assert.equal(zonedToUnix(2018, 11, 4, 0, 30), at("2018-11-04T01:30:00-02:00"));
  // "Amanhã 8h" pedido na noite anterior cai em 8h de parede do dia seguinte (já no horário de verão), não 24 h depois.
  const now = at("2018-11-03T22:00:00-03:00");
  assert.equal(find(now, "tomorrow"), at("2018-11-04T08:00:00-02:00"));
  assert.equal(find(now, "tomorrow") - now, 9 * 3600); // 22h → 8h são 10 h de parede, 9 h reais (a hora pulada)
});

test("horário de verão antigo (2019): fim 17/02 00:00 volta para 23:00 de 16/02", () => {
  assert.equal(zonedToUnix(2019, 2, 16, 20, 0), at("2019-02-16T20:00:00-02:00"));
  assert.equal(zonedToUnix(2019, 2, 17, 8, 0), at("2019-02-17T08:00:00-03:00"));
  // 23:30 de 16/02 ocorreu duas vezes: vale a primeira (-02:00).
  assert.equal(zonedToUnix(2019, 2, 16, 23, 30), at("2019-02-16T23:30:00-02:00"));
  const now = at("2019-02-16T22:00:00-02:00");
  assert.equal(find(now, "tomorrow"), at("2019-02-17T08:00:00-03:00"));
  assert.equal(find(now, "tomorrow") - now, 11 * 3600); // 22h → 8h são 10 h de parede, 11 h reais (a hora repetida)
});

test("ida e volta do campo datetime-local", () => {
  const u = at("2026-11-27T21:05:00-03:00");
  assert.equal(toLocalInput(u), "2026-11-27T21:05");
  assert.equal(fromLocalInput("2026-11-27T21:05"), u);
  assert.ok(Number.isNaN(fromLocalInput("27/11/2026 21:05")));
});

test("passado: agora e antes não valem", () => {
  const now = at("2026-11-27T10:00:00-03:00");
  assert.equal(isPast(now - 1, now), true);
  assert.equal(isPast(now, now), true);
  assert.equal(isPast(now + 1, now), false);
});
