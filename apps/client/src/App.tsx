import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowsClockwise, MagnifyingGlass, Plus, Tray, EnvelopeSimple } from "@phosphor-icons/react";
import { api, type Account, type MessageDetail, type MessageSummary } from "./api";
import { AccountDialog } from "./AccountDialog";
import { Toaster, toast } from "./toast";
import build from "./version.json";

const PAGE = 100;
const SYNC_EVERY_MS = 60_000;
const TZ = "America/Sao_Paulo";

const fmtTime = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, hour: "2-digit", minute: "2-digit" });
const fmtDay = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, day: "numeric", month: "short" });
const fmtFull = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, day: "2-digit", month: "2-digit", year: "2-digit" });
const fmtLong = new Intl.DateTimeFormat("pt-BR", { timeZone: TZ, dateStyle: "full", timeStyle: "short" });
const dayKey = new Intl.DateTimeFormat("en-CA", { timeZone: TZ });

function shortDate(unix: number) {
  const d = new Date(unix * 1000);
  const now = new Date();
  if (dayKey.format(d) === dayKey.format(now)) return fmtTime.format(d);
  if (d.getFullYear() === now.getFullYear()) return fmtDay.format(d).replace(".", "");
  return fmtFull.format(d);
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export default function App() {
  const [accounts, setAccounts] = useState<Account[] | null>(null);
  const [messages, setMessages] = useState<MessageSummary[]>([]);
  const [hasMore, setHasMore] = useState(true);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<MessageSummary[] | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<MessageDetail | null>(null);
  /** Seleção por teclado troca o conteúdo sem animação. */
  const [viaKeyboard, setViaKeyboard] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [dialog, setDialog] = useState(false);
  const [readerOpen, setReaderOpen] = useState(false);
  /** Ids que chegaram depois da primeira carga: entram com destaque. */
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  /** Cascata só na primeira abertura; buscas e atualizações trocam a lista sem animar. */
  const [intro, setIntro] = useState(true);
  const known = useRef<Set<number> | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const visible = results ?? messages;
  const unread = useMemo(() => messages.filter((m) => !m.seen).length, [messages]);

  const reload = useCallback(async (count = PAGE) => {
    const list = await api.list(Math.max(count, PAGE), 0);
    if (known.current) {
      const fresh = list.filter((m) => !known.current!.has(m.id)).map((m) => m.id);
      if (fresh.length) {
        setFreshIds(new Set(fresh));
        setTimeout(() => setFreshIds(new Set()), 2000);
      }
    }
    known.current = new Set(list.map((m) => m.id));
    setMessages(list);
    setHasMore(list.length >= Math.max(count, PAGE));
  }, []);

  const sync = useCallback(async (manual: boolean) => {
    setSyncing(true);
    try {
      const r = await api.sync();
      if (r.skipped) return;
      r.errors.forEach((e) => toast(e, "error"));
      if (r.fetched) {
        await reload(messages.length);
        toast(plural(r.fetched, "mensagem nova", "mensagens novas"), "success");
      } else if (manual && !r.errors.length) {
        toast("Caixa em dia", "info");
      }
    } catch (e) {
      toast(`Falha ao sincronizar: ${e}`, "error");
    } finally {
      setSyncing(false);
    }
  }, [reload, messages.length]);

  // Primeira carga: o banco local responde na hora; a rede vem depois.
  useEffect(() => {
    api.listAccounts().then(setAccounts);
    reload().finally(() => setTimeout(() => setIntro(false), 700));
  }, [reload]);

  const syncRef = useRef(sync);
  syncRef.current = sync;
  useEffect(() => {
    if (!accounts?.length) return;
    syncRef.current(false);
    const t = setInterval(() => syncRef.current(false), SYNC_EVERY_MS);
    return () => clearInterval(t);
  }, [accounts?.length]);

  // Busca local a cada tecla: sem debounce, o SQLite responde em < 1 ms.
  useEffect(() => {
    if (!query.trim()) return setResults(null);
    let alive = true;
    api.search(query).then((r) => alive && setResults(r));
    return () => { alive = false; };
  }, [query]);

  async function select(id: number, keyboard = false) {
    setViaKeyboard(keyboard);
    setSelectedId(id);
    setReaderOpen(true);
    const d = await api.open(id);
    setDetail(d);
    if (d) {
      const markSeen = (l: MessageSummary[]) => l.map((m) => (m.id === id ? { ...m, seen: true } : m));
      setMessages(markSeen);
      setResults((r) => r && markSeen(r));
    }
  }

  async function loadMore() {
    if (!hasMore || results) return;
    const more = await api.list(PAGE, messages.length);
    more.forEach((m) => known.current?.add(m.id));
    setMessages((l) => [...l, ...more]);
    setHasMore(more.length === PAGE);
  }

  // Atalhos: j/k ou setas navegam, / busca, r sincroniza, Esc limpa a busca ou volta.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (dialog) return;
      const typing = e.target instanceof HTMLInputElement;
      if (e.key === "Escape") {
        if (typing) { setQuery(""); searchRef.current?.blur(); }
        else setReaderOpen(false);
        return;
      }
      if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
      const idx = visible.findIndex((m) => m.id === selectedId);
      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        const next = visible[Math.min(idx + 1, visible.length - 1)];
        if (next) select(next.id, true);
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        const prev = visible[Math.max(idx - 1, 0)];
        if (prev) select(prev.id, true);
      } else if (e.key === "/") {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (e.key === "r") {
        sync(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>("[aria-selected=true]")?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  const empty = accounts !== null && accounts.length === 0;

  return (
    <div className="app" data-reader={readerOpen || undefined}>
      <aside className="rail">
        <div className="brand">Posta</div>
        <nav>
          <button className="nav-item" aria-current="page">
            <Tray size={16} weight="bold" />
            <span>Caixa de entrada</span>
            {unread > 0 && <span className="count">{unread}</span>}
          </button>
        </nav>
        <div className="rail-section">
          <div className="rail-label">Contas</div>
          {accounts?.map((a) => (
            <div key={a.id} className="account" title={a.id}>
              <span className="avatar" aria-hidden>{a.username[0]?.toUpperCase()}</span>
              <span className="account-name">{a.username}</span>
            </div>
          ))}
          <button className="btn ghost small" onClick={() => setDialog(true)}>
            <Plus size={14} weight="bold" /> Adicionar conta
          </button>
        </div>
        <div className="shortcuts" aria-label="Atalhos">
          <span><kbd>j</kbd><kbd>k</kbd> navegar</span>
          <span><kbd>/</kbd> buscar</span>
          <span><kbd>r</kbd> sincronizar</span>
        </div>
      </aside>

      <section className="list-pane">
        <div className="toolbar">
          <label className="search">
            <MagnifyingGlass size={16} weight="bold" aria-hidden />
            <input ref={searchRef} type="search" placeholder="Buscar" value={query}
              onChange={(e) => setQuery(e.target.value)} aria-label="Buscar mensagens" />
            {!query && <kbd>/</kbd>}
          </label>
          <button className="icon-btn" onClick={() => sync(true)} aria-label="Sincronizar" data-busy={syncing || undefined}
            disabled={!accounts?.length}>
            <ArrowsClockwise size={16} weight="bold" className="sync-icon" />
          </button>
          <button className="icon-btn mobile-only" onClick={() => setDialog(true)} aria-label="Adicionar conta">
            <Plus size={16} weight="bold" />
          </button>
        </div>
        {results && (
          <div className="result-meta">{plural(results.length, "resultado", "resultados")} para “{query.trim()}”</div>
        )}

        {empty ? (
          <div className="empty">
            <EnvelopeSimple size={28} weight="duotone" />
            <h2>Conecte sua primeira conta</h2>
            <p>Gmail, Outlook ou qualquer servidor IMAP. Os emails ficam salvos neste aparelho para abrir na hora.</p>
            <button className="btn primary" onClick={() => setDialog(true)}>Adicionar conta</button>
          </div>
        ) : results && results.length === 0 ? (
          <div className="empty quiet"><p>Nada encontrado.</p></div>
        ) : (
          <ul ref={listRef} className="list" role="listbox" aria-label="Mensagens" data-intro={intro || undefined}
            onScroll={(e) => {
              const el = e.currentTarget;
              if (el.scrollHeight - el.scrollTop - el.clientHeight < 400) loadMore();
            }}>
            {visible.map((m, i) => (
              <li key={m.id} role="option" aria-selected={m.id === selectedId}
                className="row" data-unread={!m.seen || undefined} data-fresh={freshIds.has(m.id) || undefined}
                style={{ "--i": Math.min(i, 12) } as React.CSSProperties}
                onClick={() => select(m.id)}>
                <span className="dot" aria-hidden />
                <span className="from">{m.from_name || m.from_addr}</span>
                <time className="date" dateTime={new Date(m.date * 1000).toISOString()}>{shortDate(m.date)}</time>
                <span className="subject">{m.subject || "(sem assunto)"}</span>
                <span className="snippet">{m.snippet}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <main className="reader">
        {detail && detail.id === selectedId ? (
          <article key={detail.id} className="message" data-instant={viaKeyboard || undefined}>
            <button className="btn ghost small back" onClick={() => setReaderOpen(false)}>
              <ArrowLeft size={14} weight="bold" /> Caixa de entrada
            </button>
            <h1>{detail.subject || "(sem assunto)"}</h1>
            <div className="meta">
              <span className="avatar large" aria-hidden>{(detail.from_name || detail.from_addr)[0]?.toUpperCase()}</span>
              <div>
                <div className="meta-from">{detail.from_name || detail.from_addr}</div>
                <div className="meta-addr">{detail.from_addr}</div>
              </div>
              <time className="meta-date">{fmtLong.format(new Date(detail.date * 1000))}</time>
            </div>
            <div className="body">{detail.body_text}</div>
          </article>
        ) : (
          <div className="reader-empty">
            <p>{messages.length ? "Selecione uma mensagem" : ""}</p>
          </div>
        )}
      </main>

      {dialog && (
        <AccountDialog
          onClose={() => setDialog(false)}
          onAdded={(a) => {
            setAccounts((l) => [...(l ?? []).filter((x) => x.id !== a.id), a]);
            toast(`Conta ${a.username} conectada`, "success");
          }}
        />
      )}
      <Toaster />
      <div className="version" aria-hidden>v{build.version} · {build.builtAt}</div>
    </div>
  );
}
