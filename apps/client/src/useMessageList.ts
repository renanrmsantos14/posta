// Camada de dados da lista: é o único lugar que sabe como a lista chega (hoje `api.list` por janela de offset).
// Se o backend ganhar paginação por cursor ou eventos de delta, só este arquivo muda.
import { useCallback, useRef, useState } from "react";
import { api, type MessageSummary, type View } from "./api";

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

export function useMessageList(folder: View, pending: React.MutableRefObject<Set<number>>, afterLoad: () => void) {
  const [messages, setMessages] = useState<MessageSummary[]>([]);
  const [results, setResults] = useState<MessageSummary[] | null>(null);
  const [hasMore, setHasMore] = useState(true);
  /** Ids que chegaram depois da primeira carga: entram com destaque. */
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  const known = useRef<Set<number> | null>(null);
  const folderRef = useRef(folder);
  folderRef.current = folder;

  /** Recarrega as primeiras `count` linhas e aplica só a diferença. */
  const reload = useCallback(async (count = PAGE) => {
    const want = Math.max(count, PAGE);
    const all = await api.list(folder, want, 0);
    if (folderRef.current !== folder) return;
    const list = all.filter((m) => !pending.current.has(m.id));
    if (known.current) {
      const seen = known.current;
      const fresh = list.filter((m) => !seen.has(m.id)).map((m) => m.id);
      if (fresh.length) {
        setFreshIds(new Set(fresh));
        setTimeout(() => setFreshIds(new Set()), 2000);
      }
    }
    known.current = new Set(list.map((m) => m.id));
    setMessages((prev) => mergeList(prev, list));
    setHasMore(all.length >= want);
    afterLoad();
  }, [folder, pending, afterLoad]);

  /** Próxima janela do banco local; devolve quantas linhas vieram. */
  const loadPage = useCallback(async (offset: number) => {
    const more = await api.list(folder, PAGE, offset);
    more.forEach((m) => known.current?.add(m.id));
    setMessages((l) => (l.length === offset ? [...l, ...more.filter((m) => !pending.current.has(m.id))] : l));
    setHasMore(more.length === PAGE);
    return more.length;
  }, [folder, pending]);

  const resetKnown = useCallback(() => { known.current = null; }, []);

  return { messages, setMessages, results, setResults, hasMore, setHasMore, freshIds, reload, loadPage, resetKnown };
}
