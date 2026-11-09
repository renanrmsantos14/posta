// Foco x Outros: heurística local, sem servidor. É "Foco" quando o remetente não parece automático
// (noreply, newsletter...) ou quando você já escreveu na conversa (há mensagem do fio em Enviados).
import type { MessageSummary } from "./api";

const BULK = /(^|[._+-])(no-?reply|nao-?responda|donotreply|do-?not-?reply|newsletter|news|notifications?|notificacoes?|mailer|mailer-daemon|marketing|ofertas?|promo(coes|tions)?|campaign|digest|alerts?|billing|atendimento|todomundo|bounce)([._+-]|@)/i;

export const isBulk = (addr: string) => BULK.test(addr);

/** Chaves `conta:fio` dos fios em que o usuário já escreveu (lista de Enviados). */
export function repliedThreads(sent: MessageSummary[]): Set<string> {
  return new Set(sent.filter((m) => m.thread_id).map((m) => `${m.account}:${m.thread_id}`));
}

export function isFocused(m: MessageSummary, replied: Set<string>): boolean {
  if (m.flagged || m.pinned) return true;
  if (m.thread_id && replied.has(`${m.account}:${m.thread_id}`)) return true;
  return !isBulk(m.from_addr);
}
