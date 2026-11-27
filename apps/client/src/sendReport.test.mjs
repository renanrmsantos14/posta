import test from "node:test";
import assert from "node:assert/strict";
import { dueMessages, scheduledStatus } from "./sendReport.ts";

const base = { sent: 0, late: 0, offline: 0, retrying: 0, failed: 0, last_error: null };

test("nada de especial não gera aviso", () => {
  assert.deepEqual(dueMessages(base), []);
  assert.deepEqual(dueMessages({ ...base, sent: 3 }), [], "saiu no horário: sem barulho");
});

test("atrasado avisa que o app estava fechado, no singular e no plural", () => {
  assert.match(dueMessages({ ...base, sent: 1, late: 1 })[0].text, /^1 mensagem agendada saiu agora/);
  assert.match(dueMessages({ ...base, sent: 2, late: 2 })[0].text, /^2 mensagens agendadas saíram agora/);
});

test("falha mostra o motivo; sem rede vai para a caixa de saída; atraso e falha não se escondem", () => {
  const m = dueMessages({ ...base, sent: 1, late: 1, failed: 1, last_error: "Oi: servidor recusou" });
  assert.equal(m.length, 2);
  assert.match(m[0].text, /saiu agora/);
  assert.equal(m[1].kind, "error");
  assert.match(m[1].text, /1 envio agendado falhou \(Oi: servidor recusou\)/);
  assert.match(dueMessages({ ...base, retrying: 2 })[0].text, /2 envios agendados não saíram/);
  assert.match(dueMessages({ ...base, offline: 1 })[0].text, /Caixa de saída/);
});

test("estado do agendado na lista", () => {
  assert.equal(scheduledStatus({ status: "pending", error: "", attempts: 0, send_at: 0 }), "Aguardando a hora");
  // 2026-11-28T16:02:00Z = 13:02 em São Paulo
  assert.match(scheduledStatus({ status: "pending", error: "sem conexão", attempts: 1, send_at: 1795881720 }), /Nova tentativa às 13:02 \(2 de 3\)/);
  assert.equal(scheduledStatus({ status: "failed", error: "recusado", attempts: 3, send_at: 0 }), "Falhou: recusado");
});
