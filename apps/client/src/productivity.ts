// Produtividade: regras, respostas prontas, férias, filtros da busca e horários de adiar/agendar. Sem dependências, para api e mock usarem.

/** Regra automática; o núcleo (Rust) lê o mesmo JSON de `settings.rules`. */
export interface Rule {
  id: string;
  field: "from" | "subject" | "to";
  contains: string;
  action: "move" | "read" | "star" | "category" | "archive" | "trash";
  /** Pasta (`move`) ou id da categoria (`category`). */
  arg: string;
  enabled: boolean;
}

export interface Template {
  id: string;
  name: string;
  body: string;
}

/** Resposta de férias; o núcleo lê o mesmo JSON de `settings.vacation`. Datas `aaaa-mm-dd`. */
export interface Vacation {
  enabled: boolean;
  start: string;
  end: string;
  subject: string;
  body: string;
}

/** Item da lista "Agendados". */
export interface ScheduledItem {
  id: string;
  account: string;
  /** Unix, em segundos. */
  send_at: number;
  status: "pending" | "sending" | "failed";
  error: string;
  to: string;
  subject: string;
}

export const FIELD_LABEL: Record<Rule["field"], string> = { from: "Remetente", subject: "Assunto", to: "Destinatário" };
export const ACTION_LABEL: Record<Rule["action"], string> = {
  move: "Mover para a pasta", read: "Marcar como lida", star: "Marcar com estrela", category: "Aplicar categoria", archive: "Arquivar", trash: "Mover para a Lixeira",
};

function parseList<T>(raw: string | undefined, ok: (x: Record<string, unknown>) => boolean): T[] {
  try {
    const list: unknown = JSON.parse(raw || "[]");
    return Array.isArray(list) ? (list.filter((x) => x && typeof x === "object" && ok(x as Record<string, unknown>)) as T[]) : [];
  } catch {
    return [];
  }
}

export const parseRules = (raw: string | undefined) =>
  parseList<Rule>(raw, (r) => typeof r.id === "string" && typeof r.field === "string" && typeof r.contains === "string" && typeof r.action === "string");

export const parseTemplates = (raw: string | undefined) =>
  parseList<Template>(raw, (t) => typeof t.id === "string" && typeof t.name === "string" && typeof t.body === "string");

export function parseVacation(raw: string | undefined): Vacation {
  const empty: Vacation = { enabled: false, start: "", end: "", subject: "", body: "" };
  try {
    const v: unknown = JSON.parse(raw || "{}");
    return v && typeof v === "object" ? { ...empty, ...(v as Partial<Vacation>) } : empty;
  } catch {
    return empty;
  }
}

// ---- Horários (ver when.ts) ----
export { fromLocalInput, isPast, toLocalInput, whenLabel, whenPresets, type WhenPreset } from "./when";

// ---- Filtros da busca ----

/** Filtros oferecidos no menu da busca: texto inserido e descrição. */
export const SEARCH_FILTERS: { token: string; label: string }[] = [
  { token: "de:", label: "De" },
  { token: "para:", label: "Para" },
  { token: "assunto:", label: "Assunto" },
  { token: "tem:anexo", label: "Com anexo" },
  { token: "é:não-lida", label: "Não lidas" },
  { token: "depois:", label: "Depois de (aaaa-mm-dd)" },
  { token: "antes:", label: "Antes de (aaaa-mm-dd)" },
];

const FILTER_KEYS = /(^|\s)(de|para|assunto|tem|é|e|antes|depois|desde|from|to|subject|has|is|before|after):\S/i;
/** A consulta usa algum filtro (`de:`, `tem:anexo`...)? */
export const hasFilters = (query: string) => FILTER_KEYS.test(query);

/** Acrescenta o filtro ao fim da consulta (sem repetir os que não aceitam valor). */
export function addFilter(query: string, token: string): string {
  const q = query.trimEnd();
  if (!token.endsWith(":") && q.split(/\s+/).includes(token)) return query;
  return q ? `${q} ${token}` : token;
}
