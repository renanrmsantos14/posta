// Camada de dados da lista: é o único lugar que sabe como a lista chega.
// Pastas reais (Role) usam paginação por cursor (`list_messages_page`) e o evento `mail-delta` (inserir/atualizar/remover por id);
// as virtuais (estrela, todas as caixas, pastas do usuário, categorias) seguem por janela de offset e recarga.
import { useCallback, useRef, useState } from "react";
import { api, type MailDelta, type MessageSummary, type View } from "./api";
import { roleOf } from "./views";

export const PAGE = 100;

const same = (a: MessageSummary, b: MessageSummary) =>
  a.id === b.id && a.seen === b.seen && a.flagged === b.flagged && a.pinned === b.pinned && a.category === b.category &&
  a.thread_count === b.thread_count && a.subject === b.subject && a.snippet === b.snippet && a.date === b.date && a.role === b.role;

/**
 * Aplica a lista nova sobre a atual reaproveitando os objetos que não mudaram.
 * Nada mudou: devolve a própria lista (React não re-renderiza); mudou pouco: só as linhas afetadas trocam de objeto.
 */
export function mergeList(prev: MessageSummary[], next: MessageSummary[]): MessageSummary[] {
  if (!prev.length) return next;
  const old = new Map(prev.map((m) => [m.id, m]));
  let identical = prev.length === next.length;
  const out = next.map((m, i) => {
    const o = old.get(m.id);
    const keep = o && same(o, m) ? o : m;
    if (identical && prev[i] !== keep) identical = false;
    return keep;
  });
  return identical ? prev : out;
}

/** Ordem da lista: fixadas primeiro, depois data e id, decrescentes. */
const before = (a: MessageSummary, b: MessageSummary) =>
  !!a.pinned !== !!b.pinned ? !!a.pinned : a.date !== b.date ? a.date > b.date : a.id > b.id;

/**
 * Aplica um delta sobre a lista carregada: tira o que saiu, troca o que mudou e insere o que chegou na posição certa.
 * `fresh` traz os resumos atuais dos ids pedidos (ausente = saiu da pasta); `asked` são os ids pedidos.
 * Uma chegada que cairia depois da última linha carregada fica de fora: a próxima página a traz.
 */
export function applyDeltaToList(prev: MessageSummary[], fresh: Map<number, MessageSummary>, asked: Set<number>, removed: Set<number>, moreBelow: boolean): MessageSummary[] {
  const next: MessageSummary[] = [];
  const placed = new Set<number>();
  /** Mudaram de posição (fixada ou data): saem e entram de novo no lugar certo. */
  const moved: MessageSummary[] = [];
  let touched = false;
  for (const m of prev) {
    if (removed.has(m.id) || (asked.has(m.id) && !fresh.has(m.id))) { touched = true; continue; }
    const f = fresh.get(m.id);
    if (!f) { next.push(m); continue; }
    placed.add(m.id);
    if (same(m, f)) { next.push(m); continue; }
    touched = true;
    if (!!f.pinned !== !!m.pinned || f.date !== m.date) moved.push(f); else next.push(f);
  }
  const arrivals = [...fresh.values()].filter((f) => !placed.has(f.id)).sort((a, b) => (before(a, b) ? -1 : 1));
  for (const m of [...moved, ...arrivals]) {
    let i = 0;
    while (i < next.length && !before(m, next[i])) i++;
    if (i === next.length && moreBelow) continue;
    next.splice(i, 0, m);
    touched = true;
  }
  return touched ? next : prev;
}

export function useMessageList(folder: View, pending: React.MutableRefObject<Set<number>>, afterLoad: () => void) {
  const [messages, setMessages] = useState<MessageSummary[]>([]);
  const [results, setResults] = useState<MessageSummary[] | null>(null);
  const [hasMore, setHasMore] = useState(true);
  /** Ids que chegaram depois da primeira carga: entram com destaque. */
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  const known = useRef<Set<number> | null>(null);
  const folderRef = useRef(folder);
  folderRef.current = folder;
  const role = roleOf(folder);
  /** Cursor da próxima página (só pastas reais); null = acabou. */
  const cursor = useRef<string | null>(null);
  /** Sobe a cada recarga: uma página que voltou depois dela pertence à lista antiga e é descartada. */
  const gen = useRef(0);
  /** Sobe a cada delta aplicado: uma página que estava em voo durante um delta pode ter vindo com dados anteriores a ele. */
  const deltaSeq = useRef(0);
  const moreRef = useRef(true);
  const setMore = (v: boolean) => { moreRef.current = v; setHasMore(v); };

  const flash = (ids: number[]) => {
    if (!ids.length) return;
    setFreshIds(new Set(ids));
    setTimeout(() => setFreshIds(new Set()), 2000);
  };

  /** Recarrega as primeiras `count` linhas e aplica só a diferença. */
  const reload = useCallback(async (count = PAGE) => {
    const want = Math.max(count, PAGE);
    const g = ++gen.current;
    let all: MessageSummary[];
    let more: boolean;
    if (role) {
      const pg = await api.listPage(role, null, want);
      if (gen.current !== g) return;
      all = pg.items;
      cursor.current = pg.next;
      more = pg.next !== null;
    } else {
      all = await api.list(folder, want, 0);
      if (gen.current !== g) return;
      more = all.length >= want;
    }
    if (folderRef.current !== folder) return;
    const list = all.filter((m) => !pending.current.has(m.id));
    if (known.current) {
      const seen = known.current;
      flash(list.filter((m) => !seen.has(m.id)).map((m) => m.id));
    }
    known.current = new Set(list.map((m) => m.id));
    setMessages((prev) => mergeList(prev, list));
    setMore(more);
    afterLoad();
  }, [folder, role, pending, afterLoad]);

  /** Próxima página (cursor nas pastas reais, offset nas virtuais); devolve quantas linhas vieram. */
  const loadPage = useCallback(async (offset: number) => {
    if (role) {
      if (cursor.current === null) return 0;
      const g = gen.current;
      const seq = deltaSeq.current;
      const pg = await api.listPage(role, cursor.current, PAGE);
      if (gen.current !== g || folderRef.current !== folder) return 0;
      cursor.current = pg.next;
      pg.items.forEach((m) => known.current?.add(m.id));
      setMessages((l) => {
        const have = new Set(l.map((m) => m.id));
        return [...l, ...pg.items.filter((m) => !have.has(m.id) && !pending.current.has(m.id))];
      });
      setMore(pg.next !== null);
      // Delta chegou com a página em voo: ela pode trazer de volta uma linha removida ou não conter uma chegada; reconcilia do início.
      if (deltaSeq.current !== seq) void reload(known.current?.size ?? PAGE);
      return pg.items.length;
    }
    const g = gen.current;
    const more = await api.list(folder, PAGE, offset);
    if (gen.current !== g || folderRef.current !== folder) return 0;
    more.forEach((m) => known.current?.add(m.id));
    setMessages((l) => (l.length === offset ? [...l, ...more.filter((m) => !pending.current.has(m.id))] : l));
    setMore(more.length === PAGE);
    return more.length;
  }, [folder, role, pending, reload]);

  /**
   * Aplica o evento `mail-delta` por id, sem recarregar a lista. Devolve false quando a pasta aberta não pagina por cursor
   * (virtuais): quem chamou cai no `mail-changed`, que recarrega.
   */
  const applyDelta = useCallback(async (d: MailDelta) => {
    if (!role) return false;
    const f = folder;
    deltaSeq.current++;
    const asked = new Set([...d.inserted, ...d.changed]);
    const removed = new Set(d.removed);
    const got = asked.size ? await api.summaries([...asked]) : [];
    if (folderRef.current !== f) return true;
    const fresh = new Map(got.filter((m) => m.role === role && !pending.current.has(m.id)).map((m) => [m.id, m]));
    const arrived = d.inserted.filter((id) => fresh.has(id) && !known.current?.has(id));
    fresh.forEach((_, id) => known.current?.add(id));
    removed.forEach((id) => known.current?.delete(id));
    setMessages((prev) => applyDeltaToList(prev, fresh, asked, removed, moreRef.current));
    flash(arrived);
    afterLoad();
    return true;
  }, [folder, role, pending, afterLoad]);

  const resetKnown = useCallback(() => { known.current = null; }, []);

  return { messages, setMessages, results, setResults, hasMore, freshIds, reload, loadPage, applyDelta, resetKnown };
}
