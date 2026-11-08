import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowBendUpLeft, ArrowsClockwise, Archive, MagnifyingGlass, NotePencil, Plus, Trash, Tray, EnvelopeSimple, GearSix } from "@phosphor-icons/react";
import { api, type Account, type MessageDetail, type MessageSummary, type Role } from "./api";
import { AccountDialog } from "./AccountDialog";
import { Composer, type ComposeInit } from "./Composer";
import { Toaster, toast } from "./toast";
import { FOLDERS } from "./folders";
import { SettingsDialog } from "./SettingsDialog";
import { UpdateDialog } from "./UpdateDialog";
import { autoCheck, checkUpdate, CHECK_EVERY_MS, skip, skipped, type Update } from "./updater";
import build from "./version.json";

const PAGE = 100;
const SYNC_EVERY_MS = 60_000;
/** Janela inicial das pastas fora da Caixa de entrada, e quanto cresce ao rolar até o fim. */
const KEEP = 1000;
const KEEP_STEP = 500;
const UNDO_MS = 4000;
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

/** Corpo da resposta: espaço para escrever no topo e a mensagem original citada. */
function replyBody(d: MessageDetail) {
  const quoted = d.body_text.trimEnd().split("\n").map((l) => `> ${l}`).join("\n");
  return `\n\nEm ${fmtLong.format(new Date(d.date * 1000))}, ${d.from_name || d.from_addr} escreveu:\n${quoted}\n`;
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
  const [compose, setCompose] = useState<ComposeInit | null>(null);
  const [settings, setSettings] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  /** Remonta o painel a cada abertura, mesmo com o mesmo rascunho. */
  const [composeKey, setComposeKey] = useState(0);
  /** Ids que chegaram depois da primeira carga: entram com destaque. */
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  /** Cascata só na primeira abertura; buscas e atualizações trocam a lista sem animar. */
  const [intro, setIntro] = useState(true);
  const [folder, setFolder] = useState<Role>("inbox");
  const [folderList, setFolderList] = useState<Role[]>(["inbox"]);
  /** Janela da pasta atual no servidor e total que o servidor informou. */
  const [keep, setKeep] = useState(KEEP);
  const [serverTotal, setServerTotal] = useState<number | null>(null);
  const [loadingFolder, setLoadingFolder] = useState(true);
  const [inboxUnread, setInboxUnread] = useState(0);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const known = useRef<Set<number> | null>(null);
  const folderRef = useRef(folder);
  folderRef.current = folder;
  const loadingMore = useRef(false);
  /** Mensagens no intervalo do "Desfazer": ainda estão no banco local, mas não devem voltar à lista. */
  const pendingMove = useRef<Set<number>>(new Set());
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const visible = results ?? messages;
  const unread = useMemo(() => messages.filter((m) => !m.seen).length, [messages]);

  const current = visible.find((m) => m.id === selectedId) ?? null;
  const archiveTo: Role = folder === "trash" || folder === "junk" ? "inbox" : "archive";
  const canArchive = folder !== "archive" && folderList.includes(archiveTo);
  const canTrash = folder === "trash" || folderList.includes("trash");

  const reload = useCallback(async (count = PAGE) => {
    const all = await api.list(folder, Math.max(count, PAGE), 0);
    if (folderRef.current !== folder) return;
    const list = all.filter((m) => !pendingMove.current.has(m.id));
    if (known.current) {
      const fresh = list.filter((m) => !known.current!.has(m.id)).map((m) => m.id);
      if (fresh.length) {
        setFreshIds(new Set(fresh));
        setTimeout(() => setFreshIds(new Set()), 2000);
      }
    }
    known.current = new Set(list.map((m) => m.id));
    setMessages(list);
    setHasMore(all.length >= Math.max(count, PAGE));
  }, [folder]);

  const syncFolder = useCallback(async (role: Role, window: number) => {
    const r = await api.syncFolder(role, window);
    r.errors.forEach((e) => toast(e, "error"));
    // Outra sincronização em andamento: não há total confiável (0 esconderia "carregar mais").
    if (!r.skipped && folderRef.current === role) setServerTotal(r.total);
    return !r.skipped;
  }, []);

  const sync = useCallback(async (manual: boolean) => {
    setSyncing(true);
    try {
      if (manual && folder !== "inbox") {
        await syncFolder(folder, keep);
        await reload(messages.length);
        return;
      }
      const r = await api.sync();
      if (r.skipped) return;
      r.errors.forEach((e) => toast(e, "error"));
      api.folders().then(setFolderList).catch(() => {});
      if (r.fetched) {
        if (folder === "inbox") await reload(messages.length);
        toast(plural(r.fetched, "mensagem nova", "mensagens novas"), "success");
      } else if (manual && !r.errors.length) {
        toast("Caixa em dia", "info");
      }
    } catch (e) {
      toast(`Falha ao sincronizar: ${e}`, "error");
    } finally {
      setSyncing(false);
    }
  }, [reload, syncFolder, folder, keep, messages.length]);

  useEffect(() => {
    api.listAccounts().then(setAccounts);
    api.folders().then(setFolderList).catch(() => {});
  }, []);

  // Carga da pasta: o banco local responde na hora; fora da Caixa de entrada, o servidor vem depois.
  useEffect(() => {
    let alive = true;
    known.current = null;
    setKeep(KEEP);
    setServerTotal(null);
    setLoadingFolder(true);
    (async () => {
      try {
        await reload();
        if (alive && folder !== "inbox") {
          setSyncing(true);
          // A sincronização periódica da Caixa de entrada pode estar rodando: espera a vez em vez de mostrar a pasta vazia.
          for (let i = 0; i < 8 && alive && !(await syncFolder(folder, KEEP)); i++) {
            await new Promise((r) => setTimeout(r, 1500));
          }
          if (alive) await reload();
        }
      } catch (e) {
        if (alive) toast(`Falha ao carregar a pasta: ${e}`, "error");
      } finally {
        if (alive) {
          setSyncing(false);
          setLoadingFolder(false);
          setTimeout(() => setIntro(false), 700);
        }
      }
    })();
    return () => { alive = false; };
  }, [reload, syncFolder, folder]);

  useEffect(() => {
    if (folder === "inbox" && !loadingFolder) setInboxUnread(unread);
  }, [folder, unread, loadingFolder]);

  // A mensagem do diálogo sumiu (ex.: excluída no servidor): fecha, senão os atalhos ficam bloqueados.
  useEffect(() => {
    if (confirmDelete && !current && !deleting) setConfirmDelete(false);
  }, [confirmDelete, current, deleting]);

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
    api.search(query, folder).then((r) => alive && setResults(r));
    return () => { alive = false; };
  }, [query, folder]);

  function openFolder(role: Role) {
    if (role === folder) return;
    setQuery("");
    setResults(null);
    setMessages([]);
    setSelectedId(null);
    setDetail(null);
    setReaderOpen(false);
    setIntro(false);
    setLoadingFolder(true);
    setFolder(role);
  }

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

  function openComposer(init: ComposeInit) {
    setComposeKey((k) => k + 1);
    setCompose(init);
  }

  function writeNew() {
    if (!accounts?.length) return setDialog(true);
    openComposer({ title: "Nova mensagem", account: accounts[0].id, reply_to: null, to: "", subject: "", body: "" });
  }

  function reply() {
    if (!detail || !accounts?.length) return;
    const subject = /^re:/i.test(detail.subject) ? detail.subject : `Re: ${detail.subject}`;
    const account = accounts.some((a) => a.id === detail.account) ? detail.account : accounts[0].id;
    openComposer({ title: "Responder", account, reply_to: detail.id, to: detail.reply_addr, subject, body: replyBody(detail) });
  }

  // O painel fecha na hora; o envio espera o "Desfazer" expirar. Se falhar, o rascunho volta intacto.
  function send(d: ComposeInit) {
    setCompose(null);
    toast("Enviando…", "sending", {
      action: { label: "Desfazer", onClick: () => openComposer({ ...d, restored: true }) },
      onClose: async (reason) => {
        if (reason === "action") return;
        const { title: _t, restored: _r, ...draft } = d;
        try {
          const r = await api.send(draft);
          toast("Mensagem enviada", "success");
          if (r.warning) toast(r.warning, "info");
        } catch (e) {
          toast(`Não foi enviada: ${e}`, "error");
          openComposer({ ...d, restored: true });
        }
      },
    });
  }

  async function loadMore() {
    if (results || loadingMore.current) return;
    loadingMore.current = true;
    try {
      if (hasMore) {
        const more = await api.list(folder, PAGE, messages.length);
        more.forEach((m) => known.current?.add(m.id));
        setMessages((l) => [...l, ...more]);
        setHasMore(more.length === PAGE);
      } else if (folder !== "inbox" && serverTotal !== null && serverTotal > keep) {
        const next = keep + KEEP_STEP;
        setKeep(next);
        await syncFolder(folder, next);
        await reload(messages.length + PAGE);
      }
    } catch (e) {
      toast(`Falha ao carregar mais: ${e}`, "error");
    } finally {
      loadingMore.current = false;
    }
  }

  /** Tira a mensagem da lista, seleciona a vizinha e devolve a função que a reinsere no mesmo lugar. */
  function takeOut(m: MessageSummary) {
    const from = folder;
    const idx = visible.findIndex((x) => x.id === m.id);
    const next = visible[idx + 1] ?? visible[idx - 1];
    const drop = (l: MessageSummary[]) => l.filter((x) => x.id !== m.id);
    setMessages(drop);
    setResults((r) => r && drop(r));
    if (next) select(next.id, true);
    else { setSelectedId(null); setDetail(null); setReaderOpen(false); }
    return () => {
      if (folderRef.current !== from) return;
      const put = (l: MessageSummary[]) => (l.some((x) => x.id === m.id) ? l : [...l.slice(0, idx), m, ...l.slice(idx)]);
      setMessages(put);
      setResults((r) => r && put(r));
      select(m.id, true);
    };
  }

  /** Sai da lista na hora; o servidor só recebe quando o "Desfazer" expira. */
  function moveOut(m: MessageSummary, to: Role) {
    const undo = takeOut(m);
    pendingMove.current.add(m.id);
    const restore = () => { pendingMove.current.delete(m.id); undo(); };
    const text = to === "trash" ? "Movida para a Lixeira" : to === "inbox" ? "Movida para a Caixa de entrada" : "Arquivada";
    toast(text, "info", {
      action: { label: "Desfazer", onClick: restore },
      duration: UNDO_MS,
      onClose: async (reason) => {
        if (reason === "action") return;
        try {
          await api.move(m.id, to);
          pendingMove.current.delete(m.id);
        } catch (e) {
          restore();
          toast(`Não foi possível mover: ${e}`, "error");
        }
      },
    });
  }

  function archive() {
    if (current && canArchive) moveOut(current, archiveTo);
  }

  function remove() {
    if (!current || !canTrash) return;
    if (folder === "trash") setConfirmDelete(true);
    else moveOut(current, "trash");
  }

  async function deleteForever() {
    if (!current || deleting) return;
    setDeleting(true);
    try {
      await api.deleteForever(current.id);
      takeOut(current);
      toast("Excluída de vez", "success");
    } catch (e) {
      toast(`Não foi possível excluir: ${e}`, "error");
    } finally {
      setDeleting(false);
      setConfirmDelete(false);
    }
  }

  // Atualização: procura ao abrir e a cada 6 h, em silêncio; só avisa se houver versão nova não adiada.
  useEffect(() => {
    const run = () => {
      if (!autoCheck()) return;
      checkUpdate()
        .then((u) => u && u.version !== skipped() && setUpdate(u))
        .catch((e) => console.warn("procurar atualização falhou:", e));
    };
    run();
    const timer = setInterval(run, CHECK_EVERY_MS);
    return () => clearInterval(timer);
  }, []);

  // Atalhos: j/k ou setas navegam, / busca, r responde, c escreve, u sincroniza, e arquiva, # exclui, Esc limpa a busca ou volta.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (dialog || compose || settings || update) return;
      if (confirmDelete) {
        if (e.key === "Escape" && !deleting) setConfirmDelete(false);
        return;
      }
      const t = e.target;
      const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
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
        e.preventDefault();
        reply();
      } else if (e.key === "c") {
        e.preventDefault();
        writeNew();
      } else if (e.key === "u") {
        sync(true);
      } else if (e.key === "e") {
        archive();
      } else if (e.key === "#") {
        remove();
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
        <button className="btn primary compose-btn" onClick={writeNew} aria-label="Escrever">
          <NotePencil size={16} weight="bold" /> <span>Escrever</span>
        </button>
        <nav>
          {folderList.map((r) => {
            const Icon = FOLDERS[r].icon;
            return (
              <button key={r} className="nav-item" title={FOLDERS[r].label} onClick={() => openFolder(r)}
                aria-current={r === folder ? "page" : undefined}>
                <Icon size={16} weight="bold" />
                <span>{FOLDERS[r].label}</span>
                {r === "inbox" && inboxUnread > 0 && <span className="count">{inboxUnread}</span>}
              </button>
            );
          })}
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
          <span><kbd>r</kbd> responder</span>
          <span><kbd>c</kbd> escrever</span>
          <span><kbd>u</kbd> sincronizar</span>
          <span><kbd>e</kbd> arquivar</span>
          <span><kbd>#</kbd> excluir</span>
        </div>
        <button className="btn ghost small settings-btn" onClick={() => setSettings(true)} aria-label="Configurações">
          <GearSix size={15} weight="bold" /> <span>Configurações</span>
        </button>
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
          <button className="icon-btn mobile-only" onClick={() => setSettings(true)} aria-label="Configurações">
            <GearSix size={16} weight="bold" />
          </button>
        </div>
        <div className="folder-tabs mobile-only" role="group" aria-label="Pastas">
          {folderList.map((r) => {
            const Icon = FOLDERS[r].icon;
            return (
              <button key={r} className="btn ghost small" onClick={() => openFolder(r)}
                aria-current={r === folder ? "page" : undefined}>
                <Icon size={14} weight="bold" /> {FOLDERS[r].label}
              </button>
            );
          })}
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
        ) : !results && visible.length === 0 ? (
          <div className="empty quiet"><p>{loadingFolder ? "Carregando…" : FOLDERS[folder].empty}</p></div>
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
              <ArrowLeft size={14} weight="bold" /> {FOLDERS[folder].label}
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
            <div className="actions">
              <button className="btn" onClick={reply}>
                <ArrowBendUpLeft size={15} weight="bold" /> Responder <kbd>r</kbd>
              </button>
              {canArchive && (
                <button className="btn" onClick={archive}>
                  {archiveTo === "inbox"
                    ? <><Tray size={15} weight="bold" /> Mover para a entrada</>
                    : <><Archive size={15} weight="bold" /> Arquivar</>} <kbd>e</kbd>
                </button>
              )}
              <button className="btn" onClick={remove} disabled={!canTrash}
                title={canTrash ? undefined : "Esta conta não tem Lixeira"}>
                <Trash size={15} weight="bold" /> {folder === "trash" ? "Excluir de vez" : "Excluir"} <kbd>#</kbd>
              </button>
            </div>
            <div className="body">{detail.body_text}</div>
          </article>
        ) : (
          <div className="reader-empty">
            <p>{messages.length ? "Selecione uma mensagem" : ""}</p>
          </div>
        )}
        {confirmDelete && current && (
          <div className="confirm scrim" onMouseDown={(e) => { if (e.target === e.currentTarget && !deleting) setConfirmDelete(false); }}>
            <div className="confirm-card" role="alertdialog" aria-labelledby="delete-title">
              <p id="delete-title">Excluir de vez?</p>
              <span>Não dá para desfazer.</span>
              <div className="confirm-actions">
                <button type="button" className="btn" onClick={() => setConfirmDelete(false)} disabled={deleting} autoFocus>Cancelar</button>
                <button type="button" className="btn danger" onClick={deleteForever} disabled={deleting}>Excluir</button>
              </div>
            </div>
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
      {compose && accounts && (
        <Composer key={composeKey} init={compose} accounts={accounts} onSend={send} onDiscard={() => setCompose(null)} />
      )}
      {settings && <SettingsDialog onClose={() => setSettings(false)} onUpdate={setUpdate} />}
      {update && (
        <UpdateDialog update={update} onLater={() => { skip(update.version); setUpdate(null); }} />
      )}
      {!compose && !dialog && !settings && !update && (
        <button className="fab mobile-only" onClick={writeNew} aria-label="Escrever">
          <NotePencil size={22} weight="bold" />
        </button>
      )}
      <Toaster />
      <div className="version" aria-hidden>v{build.version} · {build.builtAt}</div>
    </div>
  );
}
