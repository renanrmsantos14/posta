// Datas da lista e da leitura (fuso America/Sao_Paulo) e os grupos "Hoje", "Ontem"... dos cabeçalhos.
const TZ = "America/Sao_Paulo";
const fmtTime = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, hour: "2-digit", minute: "2-digit" });
const fmtDay = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, day: "numeric", month: "short" });
const fmtFull = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, day: "2-digit", month: "2-digit", year: "2-digit" });
export const fmtLong = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, dateStyle: "full", timeStyle: "short" });
const dayKey = new Intl.DateTimeFormat("en-CA", { timeZone: TZ });
const monthName = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, month: "long", year: "numeric" });

export function shortDate(unix: number) {
  const d = new Date(unix * 1000);
  const now = new Date();
  if (dayKey.format(d) === dayKey.format(now)) return fmtTime.format(d);
  if (d.getFullYear() === now.getFullYear()) return fmtDay.format(d).replace(".", "");
  return fmtFull.format(d);
}

/** Número do dia (no fuso de SP), para comparar dias inteiros. */
const dayNumber = (d: Date) => Math.floor(Date.parse(dayKey.format(d) + "T00:00:00Z") / 86_400_000);
/** Segunda-feira da semana que contém o dia `n` (1970-01-01 foi quinta). */
const weekStart = (n: number) => n - ((n + 3) % 7);

/** "Hoje" e os rótulos já calculados (por hora: o fuso de SP tem deslocamento em horas inteiras). A lista de 50 mil linhas pede um por linha. */
let today = { minute: -1, n: 0, labels: new Map<number, string>() };

/** Grupo da mensagem: Hoje, Ontem, Esta semana, Semana passada ou o mês ("Novembro de 2026"). */
export function dateGroup(unix: number, nowMs = Date.now()): string {
  const minute = Math.floor(nowMs / 60_000);
  if (today.minute !== minute) {
    const n = dayNumber(new Date(nowMs));
    today = { minute, n, labels: today.n === n && today.labels.size < 20_000 ? today.labels : new Map() };
  }
  const hour = Math.floor(unix / 3600);
  const hit = today.labels.get(hour);
  if (hit) return hit;
  const label = groupLabel(unix, today.n);
  today.labels.set(hour, label);
  return label;
}

function groupLabel(unix: number, today: number): string {
  const n = dayNumber(new Date(unix * 1000));
  if (n >= today) return "Hoje";
  if (n === today - 1) return "Ontem";
  const ws = weekStart(today);
  if (n >= ws) return "Esta semana";
  if (n >= ws - 7) return "Semana passada";
  const label = monthName.format(new Date(unix * 1000));
  return label.charAt(0).toUpperCase() + label.slice(1);
}
