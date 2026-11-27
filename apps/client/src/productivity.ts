// Produtividade: regras, respostas prontas, férias, filtros da busca e horários de adiar/agendar. Sem dependências, para api e mock usarem.

/** Campo de uma condição de regra. `attachment`: valor vazio/"yes" = tem anexo, "no" = não tem. */
export type CondField = "from" | "domain" | "subject" | "to" | "cc" | "recipients" | "attachment";
export interface Condition {
  field: CondField;
  value: string;
}

export type ActionKind = "move" | "read" | "star" | "category" | "archive" | "trash";
export interface RuleAction {
  kind: ActionKind;
  /** Pasta (`move`) ou id da categoria (`category`). */
  arg: string;
}

/**
 * Regra automática; o núcleo (Rust) lê o mesmo JSON de `settings.rules`. Todas as condições precisam casar (E);
 * a ordem da lista é a ordem de avaliação; `stop` impede as regras seguintes para a mensagem que casou.
 */
export interface Rule {
  id: string;
  name: string;
  conditions: Condition[];
  actions: RuleAction[];
  stop: boolean;
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
  /** Tentativas que já falharam. */
  attempts: number;
  to: string;
  subject: string;
}

export const FIELD_LABEL: Record<CondField, string> = {
  from: "Remetente contém", domain: "Domínio do remetente é", subject: "Assunto contém", to: "Para contém", cc: "Cc contém", recipients: "Para ou Cc contém", attachment: "Anexo",
};
export const ACTION_LABEL: Record<ActionKind, string> = {
  move: "Mover para a pasta", read: "Marcar como lida", star: "Marcar com estrela", category: "Aplicar categoria", archive: "Arquivar", trash: "Mover para a Lixeira",
};
export const ATTACHMENT_LABEL = { yes: "Tem anexo", no: "Não tem anexo" } as const;

function parseList<T>(raw: string | undefined, ok: (x: Record<string, unknown>) => boolean): T[] {
  try {
    const list: unknown = JSON.parse(raw || "[]");
    return Array.isArray(list) ? (list.filter((x) => x && typeof x === "object" && ok(x as Record<string, unknown>)) as T[]) : [];
  } catch {
    return [];
  }
}

/** Aceita o formato antigo (`field`/`contains`/`action`/`arg`) e o novo; sempre devolve o novo. */
export function parseRules(raw: string | undefined): Rule[] {
  type Raw = Partial<Rule> & { field?: string; contains?: string; action?: string; arg?: string };
  return parseList<Raw>(raw, (r) => typeof r.id === "string").map((r) => {
    const conditions = Array.isArray(r.conditions) && r.conditions.length ? r.conditions : r.field ? [{ field: r.field as CondField, value: r.contains ?? "" }] : [];
    const actions = Array.isArray(r.actions) && r.actions.length ? r.actions : r.action ? [{ kind: r.action as ActionKind, arg: r.arg ?? "" }] : [];
    return { id: r.id as string, name: r.name ?? "", conditions, actions, stop: !!r.stop, enabled: r.enabled !== false };
  });
}

export const newRuleId = () => `r${Date.now().toString(36)}${Math.random().toString(36).slice(2, 5)}`;

export const newRule = (change: Partial<Rule> = {}): Rule => ({
  id: newRuleId(), name: "", conditions: [{ field: "from", value: "" }], actions: [{ kind: "read", arg: "" }], stop: false, enabled: true, ...change,
});

/** Sem acento e sem caixa (igual ao `plain` do núcleo). */
export const plain = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

const condReady = (c: Condition) => c.field === "attachment" || plain(c.value).trim() !== "";
/** Condição que só vale preenchida e ação que precisa de destino. */
export const ruleReady = (r: Rule) =>
  r.conditions.length > 0 && r.conditions.every(condReady) && r.actions.length > 0 && r.actions.every((a) => (a.kind !== "move" && a.kind !== "category") || !!a.arg);

/** Frase curta da regra para a lista. */
export function describeRule(r: Rule, categoryName: (id: string) => string = (id) => id): string {
  const cond = r.conditions
    .map((c) => (c.field === "attachment" ? (c.value === "no" ? ATTACHMENT_LABEL.no : ATTACHMENT_LABEL.yes) : `${FIELD_LABEL[c.field] ?? c.field} “${c.value.trim() || "…"}”`))
    .join(" e ");
  const act = r.actions
    .map((a) => (a.kind === "move" ? `mover para ${a.arg || "…"}` : a.kind === "category" ? `categoria ${categoryName(a.arg)}` : ACTION_LABEL[a.kind]?.toLowerCase() ?? a.kind))
    .join(", ");
  return `${cond || "Sem condição"} → ${act || "sem ação"}${r.stop ? " (e para)" : ""}`;
}

// ---- Casamento (o mock e os avisos usam as mesmas regras do núcleo Rust) ----

export interface RuleTarget {
  from_name: string;
  from_addr: string;
  subject: string;
  to_addrs: string;
  cc_addrs: string;
  hasAttachment: boolean;
}

const domainOf = (addr: string) => (addr.includes("@") ? addr.slice(addr.lastIndexOf("@") + 1).replace(/>$/, "") : "").trim().toLowerCase();

function condMatches(c: Condition, m: RuleTarget): boolean {
  const v = plain(c.value.trim());
  switch (c.field) {
    case "attachment": return m.hasAttachment !== (c.value.trim().toLowerCase() === "no");
    case "from": return !!v && plain(`${m.from_name} ${m.from_addr}`).includes(v);
    case "subject": return !!v && plain(m.subject).includes(v);
    case "to": return !!v && plain(m.to_addrs).includes(v);
    case "cc": return !!v && plain(m.cc_addrs).includes(v);
    case "recipients": return !!v && (plain(m.to_addrs).includes(v) || plain(m.cc_addrs).includes(v));
    case "domain": {
      const d = c.value.trim().replace(/^@/, "").toLowerCase();
      const own = domainOf(m.from_addr);
      return !!d && (own === d || own.endsWith(`.${d}`));
    }
    default: return false;
  }
}

/** Regra ativa, completa e com todas as condições casando. */
export const ruleMatches = (r: Rule, m: RuleTarget) => r.enabled && ruleReady(r) && r.conditions.every((c) => condMatches(c, m));

export interface RulePlan {
  read: boolean;
  star: boolean;
  archive: boolean;
  trash: boolean;
  category: string | null;
  moveTo: string | null;
}

/** Soma as ações das regras que casam, na ordem da lista (mover e categoria: a primeira vale; `stop` encerra). */
export function planFor(rules: Rule[], m: RuleTarget): RulePlan {
  const p: RulePlan = { read: false, star: false, archive: false, trash: false, category: null, moveTo: null };
  for (const r of rules) {
    if (!ruleMatches(r, m)) continue;
    for (const a of r.actions) {
      if (a.kind === "read") p.read = true;
      else if (a.kind === "star") p.star = true;
      else if (a.kind === "archive") p.archive = true;
      else if (a.kind === "trash") p.trash = true;
      else if (a.kind === "category" && !p.category && a.arg) p.category = a.arg;
      else if (a.kind === "move" && !p.moveTo && a.arg) p.moveTo = a.arg;
    }
    if (r.stop) break;
  }
  return p;
}

// ---- Avisos da lista de regras ----

/** `a` sempre casa quando `b` casa? (por condição: mesmo campo, texto de `a` contido no de `b`) */
function implies(a: Condition, b: Condition): boolean {
  if (a.field === "attachment" || b.field === "attachment") return a.field === b.field && (a.value.trim() === "no") === (b.value.trim() === "no");
  const [x, y] = [plain(a.value.trim()), plain(b.value.trim())];
  if (!x || !y) return false;
  if (a.field === "domain" || b.field === "domain") {
    const [dx, dy] = [a.value.trim().replace(/^@/, "").toLowerCase(), b.value.trim().replace(/^@/, "").toLowerCase()];
    return a.field === b.field && (dy === dx || dy.endsWith(`.${dx}`));
  }
  const same = a.field === b.field || (a.field === "recipients" && (b.field === "to" || b.field === "cc"));
  return same && y.includes(x);
}

/** Toda mensagem que casa com `narrow` também casa com `wide`? */
const covers = (wide: Rule, narrow: Rule) => wide.conditions.every((w) => narrow.conditions.some((n) => implies(w, n)));

export interface RuleContext {
  folders: string[];
  categories: string[];
}

/** Problemas de cada regra (id → frases), para a lista avisar antes de a regra falhar em silêncio. */
export function ruleIssues(rules: Rule[], ctx: RuleContext): Map<string, string[]> {
  const out = new Map<string, string[]>();
  const add = (id: string, msg: string) => out.set(id, [...(out.get(id) ?? []), msg]);
  rules.forEach((r, i) => {
    if (!r.enabled) return;
    if (!ruleReady(r)) return add(r.id, "Incompleta: não será aplicada");
    for (const a of r.actions) {
      if (a.kind === "move" && !ctx.folders.includes(a.arg)) add(r.id, `A pasta “${a.arg}” não existe: a mensagem não será movida`);
      if (a.kind === "category" && !ctx.categories.includes(a.arg)) add(r.id, "A categoria não existe mais");
    }
    const label = (x: Rule) => `“${x.name.trim() || describeRule(x)}”`;
    for (const j of rules.slice(0, i)) {
      if (!j.enabled || !ruleReady(j) || !covers(j, r)) continue;
      if (j.stop) {
        add(r.id, `Nunca roda: a regra ${label(j)} acima já cobre estas mensagens e para as seguintes`);
        break;
      }
      const jm = j.actions.find((a) => a.kind === "move" || a.kind === "trash");
      const rm = r.actions.find((a) => a.kind === "move");
      if (jm && rm && (jm.kind === "trash" || jm.arg !== rm.arg)) {
        add(r.id, jm.kind === "trash" ? `Conflito: ${label(j)} acima manda para a Lixeira, e a Lixeira vence` : `Conflito: ${label(j)} acima já move para “${jm.arg}”; vale a primeira`);
      }
      const jc = j.actions.find((a) => a.kind === "category");
      const rc = r.actions.find((a) => a.kind === "category");
      if (jc && rc && jc.arg !== rc.arg) add(r.id, `Conflito: ${label(j)} acima já aplica outra categoria; vale a primeira`);
    }
  });
  return out;
}

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
