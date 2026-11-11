// Rodar: pnpm --filter client test (Node 22.18+ lê .ts direto).
import test from "node:test";
import assert from "node:assert/strict";
import { baseDomain, suspiciousSender as s } from "./phishing.ts";

test("positivos", () => {
  assert.match(s("Marina", "marina@lumen.com.br", "golpe@outro.com"), /diferente do remetente/);
  assert.match(s("banco@banco.com", "fraude@y.com", "fraude@y.com"), /banco@banco\.com/);
  assert.match(s("Banco", "aviso@xn--bnco-9qa.com", ""), /imitam/);
  assert.match(s("Ana", "ana@lumen.com.br", "Ana <ana@lumen.com>"), /diferente/);
});

test("negativos", () => {
  assert.equal(s("Marina", "marina@lumen.com.br", "marina@lumen.com.br"), null);
  assert.equal(s("Marina", "marina@lumen.com.br", ""), null);
  assert.equal(s("Marina", "marina@news.lumen.com.br", "<marina@lumen.com.br>"), null); // subdomínio
  assert.equal(s("marina@lumen.com.br", "noreply@mail.lumen.com.br", ""), null);
  assert.equal(s("Marina", "", "x@y.com"), null);
});

test("baseDomain", () => {
  assert.equal(baseDomain("a.b.banco.com.br"), "banco.com.br");
  assert.equal(baseDomain("mail.google.com"), "google.com");
  assert.equal(baseDomain("x.co.uk"), "x.co.uk");
});
