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

// ---- Horários (America/Sao_Paulo = UTC-3, sem horário de verão) ----

const OFFSET = 3 * 3600;
const DAY = 86_400;
const pad = (n: number) => String(n).padStart(2, "0");

/** Meia-noite (Brasília) do dia que contém `unix`. */
const startOfDay = (unix: number) => Math.floor((unix - OFFSET) / DAY) * DAY + OFFSET;
/** Dia da semana em Brasília (0 = domingo). */
const weekday = (unix: number) => new Date((unix - OFFSET) * 1000).getUTCDay();

export interface WhenPreset {
  key: string;
  label: string;
  at: number;
}

/** "Hoje mais tarde" (daqui a 3 h; depois das 20 h vira amanhã 8 h), "Amanhã" (8 h) e "Próxima semana" (segunda 8 h). */
export function whenPresets(now: number, later = "Hoje mais tarde"): WhenPreset[] {
  const today = startOfDay(now);
  const tomorrow = today + DAY + 8 * 3600;
  const evening = now - today >= 20 * 3600 - 1;
  const toMonday = (8 - weekday(now)) % 7 || 7;
  return [
    { key: "later", label: later, at: evening ? tomorrow : now + 3 * 3600 },
    { key: "tomorrow", label: "Amanhã", at: tomorrow },
    { key: "week", label: "Próxima semana", at: today + toMonday * DAY + 8 * 3600 },
  ];
}

/** Valor para `<input type="datetime-local">` (horário de Brasília). */
export function toLocalInput(unix: number): string {
  const d = new Date((unix - OFFSET) * 1000);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())}T${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** Lê o valor de `datetime-local` como horário de Brasília; NaN se inválido. */
export function fromLocalInput(v: string): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(v);
  return m ? Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]) / 1000 + OFFSET : NaN;
}

const fmtWhen = new Intl.DateTimeFormat("pt-BR", { timeZone: "America/Sao_Paulo", weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
export const whenLabel = (unix: number) => fmtWhen.format(new Date(unix * 1000));

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
