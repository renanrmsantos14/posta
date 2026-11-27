// Resumo "Hoje": o núcleo (mail-core/src/digest.rs) agrupa as últimas 24 h; aqui ficam o tipo e os rótulos dos grupos.
import type { MessageSummary } from "./api";

export type DigestKey = "important" | "people" | "notifications" | "newsletters";

export interface Digest {
  /** Janela do resumo, em segundos Unix. */
  since: number;
  until: number;
  important: MessageSummary[];
  people: MessageSummary[];
  notifications: MessageSummary[];
  newsletters: MessageSummary[];
}

/** Ordem e texto dos grupos na tela. */
export const DIGEST_GROUPS: { key: DigestKey; label: string; hint: string }[] = [
  { key: "important", label: "Importante", hint: "Com estrela, fixadas ou respostas a conversas suas" },
  { key: "people", label: "Pessoas", hint: "Quem escreveu de verdade para você" },
  { key: "notifications", label: "Notificações", hint: "Avisos, faturas e códigos" },
  { key: "newsletters", label: "Newsletters", hint: "Listas e promoções com cancelamento de inscrição" },
];

export const digestTotal = (d: Digest) => d.important.length + d.people.length + d.notifications.length + d.newsletters.length;
