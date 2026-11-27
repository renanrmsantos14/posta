// Aviso do que o relógio de envios agendados fez (evento `scheduled-report` do app). Puro: sem React nem Tauri.

export interface DueReport {
  sent: number;
  late: number;
  offline: number;
  retrying: number;
  failed: number;
  last_error: string | null;
}

export type ToastKind = "success" | "info" | "error";

const n = (v: number, one: string, many: string) => `${v} ${v === 1 ? one : many}`;

/** Avisos curtos (no máximo um por assunto): atraso, falta de rede, nova tentativa e falha. Vazio quando não há o que avisar. */
export function dueMessages(r: DueReport): { text: string; kind: ToastKind }[] {
  const out: { text: string; kind: ToastKind }[] = [];
  if (r.late > 0) {
    out.push({ text: `${n(r.late, "mensagem agendada saiu", "mensagens agendadas saíram")} agora: o app estava fechado na hora marcada.`, kind: "success" });
  }
  if (r.offline > 0) {
    out.push({ text: `Sem conexão: ${n(r.offline, "mensagem agendada foi", "mensagens agendadas foram")} para a Caixa de saída e ${r.offline === 1 ? "sai" : "saem"} quando a internet voltar.`, kind: "info" });
  }
  if (r.retrying > 0) {
    out.push({ text: `${n(r.retrying, "envio agendado não saiu", "envios agendados não saíram")}. Tento de novo em 2 minutos.`, kind: "error" });
  }
  if (r.failed > 0) {
    out.push({ text: `${n(r.failed, "envio agendado falhou", "envios agendados falharam")}${r.last_error ? ` (${r.last_error})` : ""}. Veja em Agendados.`, kind: "error" });
  }
  return out;
}

const hhmm = new Intl.DateTimeFormat("pt-BR", { hour: "2-digit", minute: "2-digit", timeZone: "America/Sao_Paulo" });

/** Texto do estado de um agendado na lista. */
export function scheduledStatus(s: { status: string; error: string; attempts: number; send_at: number }, max = 3): string {
  if (s.status === "failed") return `Falhou: ${s.error}`;
  if (s.status === "sending") return "Enviando…";
  if (s.attempts > 0) return `Não saiu (${s.error || "erro"}). Nova tentativa às ${hhmm.format(new Date(s.send_at * 1000))} (${s.attempts + 1} de ${max})`;
  return "Aguardando a hora";
}
