// Lista de mensagens virtualizada: só as linhas visíveis existem no DOM (50 mil linhas rolam sem engasgar).
// Linhas têm altura fixa por densidade; cabeçalhos de data entram na mesma lista plana. A linha (role=option) não contém
// botões: o clique em caixa/estrela é delegado por `data-act`, e as ações do mouse ficam numa barra flutuante irmã.
import { forwardRef, memo, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Archive, Check, EnvelopeSimple, EnvelopeSimpleOpen, PushPin, Star, Trash } from "@phosphor-icons/react";
import { ImportanceMark } from "./ImportanceMark";
import type { Category, MessageSummary } from "./api";
import { CategoryChip } from "./Categories";
import { dateGroup, shortDate } from "./dates";

export interface ListRow { head: MessageSummary; members: MessageSummary[] }
export type Density = "compact" | "medium" | "comfortable";

/** Altura (px) de cada linha por densidade e do cabeçalho de data. */
export const ROW_H: Record<Density, number> = { compact: 32, medium: 56, comfortable: 78 };
const HEAD_H = 30;
const OVERSCAN = 8;

type Item = { kind: "head"; key: string; label: string; top: number } | { kind: "row"; key: string; row: ListRow; index: number; top: number };

export interface ListHandle {
  /** Rola até a linha (o mínimo necessário) e, se `focus`, leva o foco do teclado até ela. */
  reveal: (id: number, focus?: boolean) => void;
}

interface Props {
  rows: ListRow[];
  grouped: boolean;
  density: Density;
  selectedId: number | null;
  checked: Set<number>;
  fresh: Set<number>;
  dragIds: Set<number>;
  popId: number | null;
  showAcct: boolean;
  acctLabel: (id: string) => string;
  acctColor: (id: string) => string;
  categories: Category[];
  intro: boolean;
  /** Agrupar por data (some em busca ou ordenação por relevância). */
  byDate: boolean;
  label: string;
  canArchive: (m: MessageSummary) => boolean;
  canTrash: (m: MessageSummary) => boolean;
  onOpen: (e: React.MouseEvent, m: MessageSummary) => void;
  onCheck: (m: MessageSummary, range: boolean) => void;
  onStar: (row: ListRow) => void;
  onContext: (x: number, y: number, row: ListRow) => void;
  /** Botão do mouse pressionado numa linha: começa a acompanhar um possível arrasto. */
  onDragStart: (e: React.PointerEvent, row: ListRow) => void;
  onDouble: () => void;
  onNearEnd: () => void;
  onScrollTop: (y: number) => void;
  swipe: (row: ListRow) => Record<string, unknown>;
  tools: { archive: (row: ListRow) => void; remove: (row: ListRow) => void; toggleRead: (row: ListRow) => void; pin: (row: ListRow) => void };
  /** Quantas linhas aparecem de cada vez (para os testes de carga). */
  onRendered?: (n: number) => void;
}

const groupOf = (r: ListRow) => (r.head.pinned ? "Fixadas" : dateGroup(r.head.date));

interface RowProps {
  item: Extract<Item, { kind: "row" }>;
  rowH: number;
  /** Sempre o `Props` mais recente: os handlers leem daqui, então a linha não precisa re-renderizar por causa deles. */
  live: React.RefObject<Props>;
  setFocusId: (id: number) => void;
  grouped: boolean;
  tab0: boolean;
  open: boolean;
  fresh: boolean;
  checked: boolean;
  dragging: boolean;
  pop: boolean;
  /** Rótulo da conta (visão "todas as contas") ou null. */
  acct: string | null;
  /** Cor da conta (faixa lateral) junto do rótulo. */
  acctColor: string | undefined;
  category: Category | undefined;
}

/** Uma linha. Memoizada: ao rolar, as linhas que continuam na janela não são recriadas (só entram as novas). */
const RowView = memo(function RowView({ item: it, rowH, live, setFocusId, grouped, tab0, open, fresh, checked, dragging, pop, acct, acctColor, category }: RowProps) {
  const { row } = it;
  const m = row.head;
  const unread = row.members.some((x) => !x.seen);
  const starred = row.members.some((x) => x.flagged);
  const n = grouped ? Math.max(row.members.length, m.thread_count) : 1;
  // Os gestos de arrastar-para-o-lado leem a ação atual na hora do evento (ela depende da configuração).
  const swipe = useMemo(() => {
    const via = (k: string) => (e: unknown) => (live.current!.swipe(row)[k] as (e: unknown) => void)(e);
    const down = via("onPointerDown");
    return { onPointerDown: (e: React.PointerEvent) => { down(e); live.current!.onDragStart(e, row); }, onPointerMove: via("onPointerMove"), onPointerUp: via("onPointerUp"), onPointerCancel: via("onPointerCancel") };
  }, [live, row]);
  return (
    <div id={`opt-${m.id}`} data-rid={m.id} role="option" aria-selected={open} tabIndex={tab0 ? 0 : -1}
      className="row" style={{ top: it.top, height: rowH, "--i": Math.min(it.index, 12) } as React.CSSProperties}
      data-unread={unread || undefined} data-fresh={fresh || undefined} data-checked={checked || undefined}
      data-dragging={dragging || undefined} data-acct-color={acctColor}
      onFocus={() => setFocusId(m.id)}
      onContextMenu={(e) => { e.preventDefault(); live.current!.onContext(e.clientX, e.clientY, row); }}
      onKeyDown={(e) => {
        if (e.key === " " && !e.ctrlKey && !e.metaKey) { e.preventDefault(); live.current!.onCheck(m, e.shiftKey); }
        else if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          live.current!.onContext(r.left + 48, r.top + r.height / 2, row);
        }
      }}
      {...swipe}
      onClick={(e) => {
        const act = (e.target as Element).closest<HTMLElement>("[data-act]")?.dataset.act;
        if (act === "check") { e.stopPropagation(); live.current!.onCheck(m, e.shiftKey); }
        else if (act === "star") { e.stopPropagation(); live.current!.onStar(row); }
        else live.current!.onOpen(e, m);
      }}
      onDoubleClick={() => live.current!.onDouble()}>
      <span className="dot" aria-hidden />
      <span className="check" data-act="check" aria-hidden><Check size={11} weight="bold" /></span>
      <span className="from has-meta">
        <span className="sr">{unread ? "Não lida. " : ""}{starred ? "Com estrela. " : ""}{m.pinned ? "Fixada. " : ""}{m.importance === 1 ? "Importância alta. " : m.importance === -1 ? "Importância baixa. " : ""}</span>
        <span className="from-name">{m.from_name || m.from_addr}</span>
        {n > 1 && <span className="thread-n" title={`${n} mensagens na conversa`}>{n}</span>}
      </span>
      <time className="date" dateTime={new Date(m.date * 1000).toISOString()}>{shortDate(m.date)}</time>
      <span className="star" data-act="star" data-on={starred || undefined} data-pop={pop || undefined} aria-hidden>
        <Star size={15} weight={starred ? "fill" : "bold"} />
      </span>
      <span className="subject">
        {acct !== null && <span className="acct" title={m.account}>{acct}</span>}
        {m.pinned && <PushPin size={12} weight="fill" className="pin-mark" aria-hidden />}
        <ImportanceMark value={m.importance} />
        {category && <CategoryChip category={category} />}
        {m.subject || "(sem assunto)"}
      </span>
      <span className="snippet">{m.snippet}</span>
    </div>
  );
});

export const MessageList = forwardRef<ListHandle, Props>(function MessageList(p, ref) {
  const box = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState(0);
  const [height, setHeight] = useState(600);
  const [focusId, setFocusId] = useState<number | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const rowH = ROW_H[p.density];
  const live = useRef(p);
  live.current = p;

  // Lista plana (cabeçalho + linhas) com a posição de cada item; recalculada só quando linhas ou densidade mudam.
  const { items, total } = useMemo(() => {
    const out: Item[] = [];
    let y = 0;
    let last = "";
    p.rows.forEach((row, index) => {
      if (p.byDate) {
        const g = groupOf(row);
        if (g !== last) { out.push({ kind: "head", key: `h:${g}`, label: g, top: y }); y += HEAD_H; last = g; }
      }
      out.push({ kind: "row", key: String(row.head.id), row, index, top: y });
      y += rowH;
    });
    return { items: out, total: y };
  }, [p.rows, p.byDate, rowH]);

  const byId = useMemo(() => new Map(items.flatMap((it) => (it.kind === "row" ? it.row.members.map((m) => [m.id, it] as const) : []))), [items]);

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setHeight(el.clientHeight);
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Janela visível por busca binária nas posições.
  const first = useMemo(() => {
    let lo = 0, hi = items.length - 1, ans = 0;
    const from = scroll - OVERSCAN * rowH;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (items[mid].top + (items[mid].kind === "head" ? HEAD_H : rowH) >= from) { ans = mid; hi = mid - 1; } else lo = mid + 1;
    }
    return ans;
  }, [items, scroll, rowH]);
  const last = useMemo(() => {
    const to = scroll + height + OVERSCAN * rowH;
    let lo = first, hi = items.length - 1, ans = first;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (items[mid].top <= to) { ans = mid; lo = mid + 1; } else hi = mid - 1;
    }
    return ans;
  }, [items, scroll, height, first, rowH]);

  const focusItem = focusId !== null ? byId.get(focusId) : undefined;
  const visible = items.slice(first, last + 1);
  if (focusItem && focusItem.kind === "row" && !visible.includes(focusItem)) visible.push(focusItem);

  useEffect(() => { p.onRendered?.(visible.length); });

  const raf = useRef(0);
  // Sem lista (pasta vazia, Rascunhos, Agendados) não há rolagem para trazer o Escrever de volta.
  const { onScrollTop } = p;
  useEffect(() => () => onScrollTop(0), [onScrollTop]);
  const onScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    cancelAnimationFrame(raf.current);
    raf.current = requestAnimationFrame(() => setScroll(el.scrollTop));
    p.onScrollTop(el.scrollTop);
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 600) p.onNearEnd();
  };

  const reveal = useCallback((id: number, focus = false) => {
    const el = box.current;
    const it = byId.get(id);
    if (!el || !it || it.kind !== "row") return;
    const top = it.top;
    // Cabeçalho logo acima da primeira linha de um grupo fica visível junto.
    const prev = items[items.indexOf(it) - 1];
    const min = prev && prev.kind === "head" ? prev.top : top;
    if (min < el.scrollTop) el.scrollTop = min;
    else if (top + rowH > el.scrollTop + el.clientHeight) el.scrollTop = top + rowH - el.clientHeight;
    setScroll(el.scrollTop);
    if (focus) {
      setFocusId(it.row.head.id);
      requestAnimationFrame(() => box.current?.querySelector<HTMLElement>(`[data-rid="${it.row.head.id}"]`)?.focus({ preventScroll: true }));
    }
  }, [byId, items, rowH]);
  useImperativeHandle(ref, () => ({ reveal }), [reveal]);

  // Quando a lista encolhe (exclusão), mantém o scroll dentro do novo tamanho.
  useEffect(() => {
    const el = box.current;
    if (el && el.scrollTop > Math.max(0, total - el.clientHeight)) { el.scrollTop = Math.max(0, total - el.clientHeight); setScroll(el.scrollTop); }
  }, [total]);

  // Poucas linhas para a altura da janela (aba Foco filtrando, exclusões): sem rolagem não há evento, então pede a próxima página.
  useEffect(() => {
    const el = box.current;
    if (el && total - el.clientHeight < 600) p.onNearEnd();
  }, [total, height]);

  const zeroId = focusId ?? p.selectedId ?? (items.find((i) => i.kind === "row") as Extract<Item, { kind: "row" }> | undefined)?.row.head.id ?? null;

  // Sem a linha de foco renderizada (rolou para longe), a própria lista precisa ser alcançável pelo teclado.
  const zeroVisible = visible.some((it) => it.kind === "row" && it.row.head.id === zeroId);

  const hoverItem = hover !== null ? byId.get(hover) : undefined;
  const selecting = p.checked.size > 0;

  return (
    <div className="list-wrap"
      onPointerMove={(e) => {
        if (e.pointerType !== "mouse") return;
        const t = e.target as Element;
        const el = t.closest<HTMLElement>("[data-rid]");
        if (el) setHover(Number(el.dataset.rid));
        else if (!t.closest(".row-tools")) setHover(null);
      }}
      onPointerLeave={() => setHover(null)}>
    <div ref={box} className="list" role="listbox" aria-label={p.label} aria-multiselectable tabIndex={zeroVisible ? undefined : 0} data-density={p.density}
      data-intro={p.intro || undefined} data-selecting={selecting || undefined}
      onScroll={onScroll}>
      <div className="list-spacer" style={{ height: total }} role="presentation">
        {visible.map((it) => {
          if (it.kind === "head") {
            return <div key={it.key} className="group-h" role="presentation" style={{ top: it.top, height: HEAD_H }}>{it.label}</div>;
          }
          const { row } = it;
          const id = row.head.id;
          return (
            <RowView key={it.key} item={it} rowH={rowH} live={live} setFocusId={setFocusId} grouped={p.grouped}
              tab0={zeroId === id} open={row.members.some((x) => x.id === p.selectedId)} fresh={p.fresh.has(id)} checked={p.checked.has(id)}
              dragging={row.members.some((x) => p.dragIds.has(x.id))} pop={p.popId === id}
              acct={p.showAcct ? p.acctLabel(row.head.account) : null} acctColor={p.showAcct ? p.acctColor(row.head.account) : undefined} category={p.categories.find((c) => c.id === row.head.category)} />
          );
        })}
      </div>
    </div>
      {hoverItem && hoverItem.kind === "row" && !selecting && (
        <div className="row-tools" role="toolbar" aria-label="Ações da mensagem" style={{ top: hoverItem.top - scroll, height: rowH }}>
          <button type="button" className="icon-btn" aria-label="Excluir" title="Excluir (Delete)" disabled={!hoverItem.row.members.some(p.canTrash)}
            onClick={() => p.tools.remove(hoverItem.row)}><Trash size={15} weight="bold" /></button>
          <button type="button" className="icon-btn" aria-label="Arquivar" title="Arquivar (e)" disabled={!hoverItem.row.members.some(p.canArchive)}
            onClick={() => p.tools.archive(hoverItem.row)}><Archive size={15} weight="bold" /></button>
          {hoverItem.row.members.some((x) => !x.seen) ? (
            <button type="button" className="icon-btn" aria-label="Marcar como lida" title="Marcar como lida (Ctrl+Q)"
              onClick={() => p.tools.toggleRead(hoverItem.row)}><EnvelopeSimpleOpen size={15} weight="bold" /></button>
          ) : (
            <button type="button" className="icon-btn" aria-label="Marcar como não lida" title="Marcar como não lida (Ctrl+U)"
              onClick={() => p.tools.toggleRead(hoverItem.row)}><EnvelopeSimple size={15} weight="bold" /></button>
          )}
          <button type="button" className="icon-btn" aria-label={hoverItem.row.head.pinned ? "Desafixar" : "Fixar no topo"} title="Fixar no topo (p)"
            onClick={() => p.tools.pin(hoverItem.row)}><PushPin size={15} weight={hoverItem.row.head.pinned ? "fill" : "bold"} /></button>
        </div>
      )}
    </div>
  );
});
