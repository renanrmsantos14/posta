// Horários de "Adiar" e "Enviar mais tarde". Puro (sem React) para rodar nos testes do Node.
// O fuso é America/Sao_Paulo calculado por IANA (Intl), não por offset fixo: o Brasil não tem horário de verão desde 2019,
// mas datas anteriores (e uma eventual volta do horário de verão) continuam certas.

export const TZ = "America/Sao_Paulo";

export interface Parts { y: number; m: number; d: number; h: number; mi: number; wd: number }

const fmtParts = new Map<string, Intl.DateTimeFormat>();
const WEEKDAYS: Record<string, number> = { Sun: 0, Mon: 1, Tue: 2, Wed: 3, Thu: 4, Fri: 5, Sat: 6 };

/** Data e hora de parede de `unix` no fuso `tz` (`wd`: 0 = domingo). */
export function partsOf(unix: number, tz = TZ): Parts {
  let f = fmtParts.get(tz);
  if (!f) {
    f = new Intl.DateTimeFormat("en-US", { timeZone: tz, hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric", weekday: "short" });
    fmtParts.set(tz, f);
  }
  const o: Record<string, string> = {};
  for (const p of f.formatToParts(new Date(unix * 1000))) o[p.type] = p.value;
  return { y: +o.year, m: +o.month, d: +o.day, h: +o.hour, mi: +o.minute, wd: WEEKDAYS[o.weekday] };
}

/** Diferença (segundos) entre a hora de parede de `tz` e UTC em `unix`. */
function offsetAt(unix: number, tz: string): number {
  const p = partsOf(unix, tz);
  return Date.UTC(p.y, p.m - 1, p.d, p.h, p.mi) / 1000 - Math.floor(unix / 60) * 60;
}

/** Unix do horário de parede `y-m-d h:mi` em `tz`. Dia/mês fora da faixa são normalizados (ex.: dia 32). Horário que não existe
 *  (início do horário de verão) vai para depois do salto; horário repetido (fim) vale a primeira ocorrência. */
export function zonedToUnix(y: number, m: number, d: number, h: number, mi: number, tz = TZ): number {
  const wall = Date.UTC(y, m - 1, d, h, mi) / 1000;
  const first = wall - offsetAt(wall - offsetAt(wall, tz), tz);
  // Se o resultado, convertido de volta, não bate com o horário pedido, o horário caiu num salto: usa o offset de depois.
  const back = partsOf(first, tz);
  if (back.h === h && back.mi === mi) return first;
  return wall - offsetAt(first, tz);
}

export interface WhenPreset { key: string; label: string; at: number }

/** Atalhos: "Hoje à noite" (20h; só até as 20h), "Amanhã" (8h) e "Próxima segunda" (8h; sempre a da semana seguinte se hoje é segunda). */
export function whenPresets(now: number, tz = TZ): WhenPreset[] {
  const t = partsOf(now, tz);
  const list: WhenPreset[] = [];
  const tonight = zonedToUnix(t.y, t.m, t.d, 20, 0, tz);
  if (tonight > now) list.push({ key: "tonight", label: "Hoje à noite", at: tonight });
  list.push({ key: "tomorrow", label: "Amanhã", at: zonedToUnix(t.y, t.m, t.d + 1, 8, 0, tz) });
  const toMonday = (8 - t.wd) % 7 || 7;
  list.push({ key: "monday", label: "Próxima segunda", at: zonedToUnix(t.y, t.m, t.d + toMonday, 8, 0, tz) });
  return list;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** Valor para `<input type="datetime-local">` no fuso `tz`. */
export function toLocalInput(unix: number, tz = TZ): string {
  const p = partsOf(unix, tz);
  return `${p.y}-${pad(p.m)}-${pad(p.d)}T${pad(p.h)}:${pad(p.mi)}`;
}

/** Lê o valor de `datetime-local` como horário de `tz`; NaN se inválido. */
export function fromLocalInput(v: string, tz = TZ): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(v);
  return m ? zonedToUnix(+m[1], +m[2], +m[3], +m[4], +m[5], tz) : NaN;
}

const fmtWhen = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
export const whenLabel = (unix: number) => fmtWhen.format(new Date(unix * 1000));

/** A hora já passou (ou é agora): adiar ou agendar para ela não faz sentido. */
export const isPast = (unix: number, now = Math.floor(Date.now() / 1000)) => !(unix > now);
