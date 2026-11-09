import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowsIn, ArrowsOut, ArrowBendUpLeft, ArrowBendDoubleUpLeft, ArrowBendUpRight, ArrowsClockwise, Archive, CaretUp, Check, MagnifyingGlass, NotePencil, Plus, Star, Trash, Tray, EnvelopeSimple, EnvelopeSimpleOpen, GearSix, Paperclip, FileText, FloppyDisk, Printer, X } from "@phosphor-icons/react";
import { HtmlBody, hasRemoteImages } from "./HtmlBody";
import { api, isTauri, type Account, type Attachment, type MessageDetail, type MessageSummary, type OutAttachment, type Role, type Settings, type StoredDraft, type UnreadCounts, type UserFolder, type View } from "./api";
import { AccountDialog } from "./AccountDialog";
import { CategoryChip, CategoryPicker, parseCategories } from "./Categories";
import { FolderDialog, type FolderDialogState } from "./FolderDialog";
import { FolderTree } from "./FolderTree";
import { boxView, catView, parseBox, parseCat } from "./views";
import { Composer, type ComposeInit } from "./Composer";
import { aliasFor, aliasLines, addrOf } from "./aliases";
import { justSwiped, swipeProps, type SwipeAction, type SwipeDir } from "./swipe";
import { escapeHtml, textToHtml } from "./richtext";
import { Toaster, toast } from "./toast";
import { FOLDERS, LOCAL_VIEWS, SHOWS_UNREAD, folderTree, viewInfo, withStarred, type Dest, type FolderNode } from "./folders";
import { fmtSize } from "./format";
import { Resizer } from "./Resizer";
import { SettingsPage } from "./SettingsPage";
import { UpdateDialog } from "./UpdateDialog";
import { autoCheck, checkUpdate, CHECK_EVERY_MS, skip, skipped, type Update } from "./updater";
import build from "./version.json";

const PAGE = 100;
/** O WebView do Android não tem impressão por `window.print`. */
const ANDROID = /Android/i.test(navigator.userAgent);
// Rede de segurança: a chegada normal vem do IDLE (evento `mail-changed`).
const SYNC_EVERY_MS = 300_000;
/** Janela inicial das pastas fora da Caixa de entrada, e quanto cresce ao rolar até o fim. */
const KEEP = 1000;
const KEEP_STEP = 500;
const UNDO_MS = 4000;
const TZ = "America/Sao_Paulo";
/** Larguras padrão e limites das colunas (px); o resto da janela fica com a leitura. */
const RAIL = { def: 232, min: 180, max: 360 };
const LIST = { def: 360, min: 280, max: 720 };
const LIST_H = { def: 320, min: 140 };
const MIN_READER = 360;

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

/** Original citado com a formatação que ele tinha (o Composer limpa o HTML); sem HTML, o texto. */
const quoted = (d: MessageDetail) => `<blockquote>${d.body_html || textToHtml(d.body_text.trimEnd())}</blockquote>`;

/** Corpo da resposta: espaço para escrever no topo e a mensagem original citada. */
function replyHtml(d: MessageDetail) {
  const who = escapeHtml(d.from_name || d.from_addr);
  return `<div><br></div><div><br></div><div>Em ${fmtLong.format(new Date(d.date * 1000))}, ${who} escreveu:</div>${quoted(d)}`;
}

/** Corpo do encaminhamento: cabeçalho da original e a mensagem citada. */
function forwardHtml(d: MessageDetail) {
  const head = [
    "---------- Mensagem encaminhada ----------",
    `De: ${d.from_name ? `${d.from_name} <${d.from_addr}>` : d.from_addr}`,
    `Data: ${fmtLong.format(new Date(d.date * 1000))}`,
    `Assunto: ${d.subject || "(sem assunto)"}`,
    d.to_addrs && `Para: ${d.to_addrs}`,
    d.cc_addrs && `Cc: ${d.cc_addrs}`,
  ].filter(Boolean).join("\n");
  return `<div><br></div><div><br></div>${textToHtml(head)}<div><br></div>${quoted(d)}`;
}

/** Para onde "Arquivar" leva: de Lixeira/Spam volta à entrada. */
const archiveTo = (m: MessageSummary): Role => (m.role === "trash" || m.role === "junk" ? "inbox" : "archive");

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** Uma linha da lista: a conversa (ou a mensagem solta). `head` é a mais recente; `members` são as mensagens dela nesta lista. */
interface Row { head: MessageSummary; members: MessageSummary[] }

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
  /** Mensagens cujas imagens remotas o usuário liberou (só nesta sessão). */
  const [imagesOk, setImagesOk] = useState<Set<number>>(new Set());
  const [syncing, setSyncing] = useState(false);
  const [dialog, setDialog] = useState(false);
  const [readerOpen, setReaderOpen] = useState(false);
  const [compose, setCompose] = useState<ComposeInit | null>(null);
  const [savingAtt, setSavingAtt] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  /** Só a mensagem, ocupando a janela (clique duplo, botão expandir ou painel desligado). */
  const [full, setFull] = useState(false);
  /** As preferências chegam do backend: até lá a janela fica invisível para não piscar o tema. */
  const [prefsReady, setPrefsReady] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  /** Remonta o painel a cada abertura, mesmo com o mesmo rascunho. */
  const [composeKey, setComposeKey] = useState(0);
  /** Ids que chegaram depois da primeira carga: entram com destaque. */
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  /** Cascata só na primeira abertura; buscas e atualizações trocam a lista sem animar. */
  const [intro, setIntro] = useState(true);
  const [folder, setFolder] = useState<View>("inbox");
  const [folderList, setFolderList] = useState<(Role | "starred")[]>(["inbox", "starred"]);
  /** Janela da pasta atual no servidor e total que o servidor informou. */
  const [keep, setKeep] = useState(KEEP);
  const [serverTotal, setServerTotal] = useState<number | null>(null);
  const [loadingFolder, setLoadingFolder] = useState(true);
  const [counts, setCounts] = useState<UnreadCounts>({});
  /** Seleção múltipla (checkbox / Shift+clique). */
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const anchor = useRef<number | null>(null);
  /** Estrela que acabou de ser clicada com o mouse: só ela anima. */
  const [popId, setPopId] = useState<number | null>(null);
  /** Mensagens da Lixeira à espera de "Excluir de vez?". */
  const [confirmDelete, setConfirmDelete] = useState<MessageSummary[] | null>(null);
  /** Rascunhos locais, a configuração (assinaturas, notificações) e os filtros de visão. */
  const [drafts, setDrafts] = useState<StoredDraft[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const [showDrafts, setShowDrafts] = useState(false);
  /** Conta escolhida na barra lateral; null = todas as caixas juntas. */
  const [accountFilter, setAccountFilter] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  /** Mensagens do fio aberto (da mais antiga à mais nova), as já abertas por expansão e quais estão expandidas. */
  const [thread, setThread] = useState<MessageSummary[]>([]);
  const [bodies, setBodies] = useState<Record<number, MessageDetail>>({});
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  /** Pastas do usuário (todas as contas), o diálogo de criar/renomear e a exclusão à espera de confirmação. */
  const [userFolders, setUserFolders] = useState<UserFolder[]>([]);
  const [folderDialog, setFolderDialog] = useState<FolderDialogState | null>(null);
  const [folderDelete, setFolderDelete] = useState<{ node: FolderNode; count: number } | null>(null);
  const [folderBusy, setFolderBusy] = useState(false);
  /** Arrastar da lista para uma pasta: mensagens em arrasto e alvo sob o ponteiro. */
  const dragging = useRef<MessageSummary[]>([]);
  const [dragIds, setDragIds] = useState<Set<number>>(new Set());
  const [dropKey, setDropKey] = useState<string | null>(null);
  const known = useRef<Set<number> | null>(null);
  const folderRef = useRef(folder);
  /** Verdadeiro enquanto uma ação de deslizar roda (takeOut não troca a seleção). */
  const swiping = useRef(false);
  folderRef.current = folder;
  const loadingMore = useRef(false);
  /** Mensagens no intervalo do "Desfazer": ainda estão no banco local, mas não devem voltar à lista. */
  const pendingMove = useRef<Set<number>>(new Set());
  const appRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const grouped = settings.group !== "0";
  const shown = results ?? messages;
  // "Todas as caixas" ignora o filtro de conta: é justamente a Entrada de todas juntas.
  const visible = useMemo(() => (accountFilter && folder !== "all" ? shown.filter((m) => m.account === accountFilter) : shown), [shown, accountFilter, folder]);
  const draftsShown = useMemo(() => (accountFilter ? drafts.filter((d) => d.account === accountFilter) : drafts), [drafts, accountFilter]);
  /** Conversas: mensagens do mesmo fio e da mesma conta viram uma linha (a lista já vem da mais nova para a mais velha). */
  const rows = useMemo<Row[]>(() => {
    if (!grouped) return visible.map((m) => ({ head: m, members: [m] }));
    const byThread = new Map<string, Row>();
    const out: Row[] = [];
    for (const m of visible) {
      const key = m.thread_id ? `${m.account}:${m.thread_id}` : "";
      const row = key ? byThread.get(key) : undefined;
      if (row) { row.members.push(m); continue; }
      const created = { head: m, members: [m] };
      if (key) byThread.set(key, created);
      out.push(created);
    }
    return out;
  }, [visible, grouped]);
  const rowOf = useMemo(() => new Map(rows.flatMap((r) => r.members.map((m) => [m.id, r] as const))), [rows]);
  /** Ações valem para o fio todo: troca cada mensagem pelas do seu fio nesta lista. */
  const withThread = (ms: MessageSummary[]) => [...new Map(ms.flatMap((m) => rowOf.get(m.id)?.members ?? [m]).map((m) => [m.id, m])).values()];
  const checkedRows = useMemo(() => rows.filter((r) => checked.has(r.head.id)), [rows, checked]);
  const checkedList = useMemo(() => checkedRows.flatMap((r) => r.members), [checkedRows]);
  /** Só a pasta real sincroniza com o servidor; "Com estrela", "Todas as caixas" e categorias são visões do banco local. */
  const remote = !LOCAL_VIEWS.includes(folder) && !parseCat(folder);
  /** "Todas as caixas" só existe com 2+ contas. */
  const navFolders: (Role | "starred" | "all")[] = accounts && accounts.length > 1 ? ["all", ...folderList] : folderList;
  const unreadOf = (r: Role | "starred" | "all") => (r === "all" ? counts.inbox : r !== "starred" && SHOWS_UNREAD.includes(r) ? counts[r] : 0);
  /** Rótulo curto da conta na linha: a parte antes do @, ou o endereço inteiro se duas contas começam igual. */
  const acctLabel = (id: string) => {
    const a = accounts?.find((x) => x.id === id);
    if (!a) return id;
    const local = a.username.split("@")[0];
    return accounts!.some((x) => x.id !== id && x.username.split("@")[0] === local) ? a.username : local;
  };
  const categories = useMemo(() => parseCategories(settings.categories), [settings.categories]);
  const tree = useMemo(() => folderTree(userFolders.filter((f) => !accountFilter || f.account === accountFilter)), [userFolders, accountFilter]);
  const info = viewInfo(folder, userFolders, categories);

  const current = visible.find((m) => m.id === selectedId) ?? null;
  const pane = settings.pane === "bottom" || settings.pane === "off" ? settings.pane : "right";
  const size = (key: string, d: number) => (Number(settings[key]) > 0 ? Number(settings[key]) : d);
  const railW = size("w:rail", RAIL.def);
  const listW = size("w:list", LIST.def);
  const listH = size("h:list", LIST_H.def);
  const layoutVars = {
    ...(settings["w:rail"] && { "--rail-pref": `${railW}px` }),
    ...(settings["w:list"] && { "--list-pref": `${listW}px` }),
    ...(settings["h:list"] && { "--list-h-pref": `${listH}px` }),
  } as React.CSSProperties;
  /** Largura máxima da lista: deixa espaço mínimo para a leitura. */
  const listMax = () => Math.max(LIST.min, Math.min(LIST.max, (appRef.current?.clientWidth ?? 1200) - (appRef.current?.querySelector<HTMLElement>(".rail")?.offsetWidth ?? RAIL.def) - MIN_READER));
  const listHMax = () => Math.max(LIST_H.min, (appRef.current?.clientHeight ?? 800) - 220);
  const canArchive = (m: MessageSummary) => m.role !== "archive" && folderList.includes(archiveTo(m));
  const canTrash = (m: MessageSummary) => m.role === "trash" || folderList.includes("trash");

  const refreshCounts = useCallback(() => {
    api.unreadCounts().then(setCounts).catch(() => {});
  }, []);
  /** Ajusta o contador na hora; `refreshCounts` confere com o banco depois. */
  const bumpCounts = (ms: MessageSummary[], delta: number) =>
    setCounts((c) => {
      const next = { ...c };
      for (const m of ms) if (m.role) next[m.role] = Math.max(0, (next[m.role] ?? 0) + delta);
      return next;
    });

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
    refreshCounts();
  }, [folder, refreshCounts]);

  const syncFolder = useCallback(async (view: View, window: number) => {
    const r = await api.syncFolder(view, window);
    r.errors.forEach((e) => toast(e, "error"));
    // Outra sincronização em andamento: não há total confiável (0 esconderia "carregar mais").
    if (!r.skipped && folderRef.current === view) setServerTotal(r.total);
    return !r.skipped;
  }, []);

  const sync = useCallback(async (manual: boolean) => {
    setSyncing(true);
    try {
      if (manual && remote) {
        await syncFolder(folder as Role, keep);
        await reload(messages.length);
        return;
      }
      const r = await api.sync();
      if (r.skipped) return;
      r.errors.forEach((e) => toast(e, "error"));
      api.folders().then((r) => setFolderList(withStarred(r))).catch(() => {});
      api.userFolders().then(setUserFolders).catch(() => {});
      if (r.fetched) {
        if (!remote) await reload(messages.length);
        toast(plural(r.fetched, "mensagem nova", "mensagens novas"), "success");
      } else if (manual && !r.errors.length) {
        toast("Caixa em dia", "info");
      }
    } catch (e) {
      toast(`Falha ao sincronizar: ${e}`, "error");
    } finally {
      setSyncing(false);
    }
  }, [reload, syncFolder, folder, remote, keep, messages.length]);

  const refreshDrafts = useCallback(() => api.listDrafts().then(setDrafts).catch(() => {}), []);

  useEffect(() => {
    api.listAccounts().then(setAccounts);
    api.folders().then((r) => setFolderList(withStarred(r))).catch(() => {});
    api.userFolders().then(setUserFolders).catch(() => {});
    refreshDrafts();
    api.settings().then((s) => {
      setSettings(s);
      // Android 13+ só mostra notificações depois do aceite; no desktop a chamada é inofensiva.
      if (s.notify !== "0") api.listAccounts().then((a) => (a.length ? api.requestNotifyPermission() : false)).catch(() => {});
    }).catch(() => {}).finally(() => setPrefsReady(true));
  }, [refreshDrafts]);

  // Fechar ou enviar o painel muda os rascunhos salvos.
  useEffect(() => { if (!compose) refreshDrafts(); }, [compose, refreshDrafts]);

  // O backend só notifica com a janela sem foco: avisa a cada mudança de foco ou visibilidade.
  useEffect(() => {
    const report = () => api.setFocused(document.hasFocus() && document.visibilityState === "visible").catch(() => {});
    report();
    window.addEventListener("focus", report);
    window.addEventListener("blur", report);
    document.addEventListener("visibilitychange", report);
    return () => {
      window.removeEventListener("focus", report);
      window.removeEventListener("blur", report);
      document.removeEventListener("visibilitychange", report);
    };
  }, []);

  // Contagem de não lidas no título da janela.
  useEffect(() => {
    const inboxUnread = counts.inbox ?? 0;
    const title = inboxUnread > 0 ? `(${inboxUnread}) Posta` : "Posta";
    document.title = title;
    if (!isTauri) return;
    import("@tauri-apps/api/window").then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title)).catch(() => {});
  }, [counts.inbox]);

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
        if (alive && remote) {
          setSyncing(true);
          // A sincronização periódica da Caixa de entrada pode estar rodando: espera a vez em vez de mostrar a pasta vazia.
          for (let i = 0; i < 8 && alive && !(await syncFolder(folder as Role, KEEP)); i++) {
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
  }, [reload, syncFolder, folder, remote]);

  // As mensagens do diálogo sumiram (ex.: excluídas no servidor): fecha, senão os atalhos ficam bloqueados.
  useEffect(() => {
    if (confirmDelete && !deleting && confirmDelete.every((c) => !visible.some((m) => m.id === c.id))) setConfirmDelete(null);
  }, [confirmDelete, visible, deleting]);

  const syncRef = useRef(sync);
  syncRef.current = sync;
  useEffect(() => {
    if (!accounts?.length) return;
    syncRef.current(false);
    const t = setInterval(() => syncRef.current(false), SYNC_EVERY_MS);
    // Android suspende o app em segundo plano: ao voltar, sincroniza na hora.
    const onVisible = () => document.visibilityState === "visible" && syncRef.current(false);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(t);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [accounts?.length]);

  // IDLE no backend: avisa na hora que algo mudou na caixa de entrada.
  const onMailChanged = useRef(async (_fetched: number) => {});
  onMailChanged.current = async (fetched: number) => {
    api.folders().then((r) => setFolderList(withStarred(r))).catch(() => {});
    if (!LOCAL_VIEWS.includes(folderRef.current)) return;
    await reload(messages.length);
    if (fetched) toast(plural(fetched, "mensagem nova", "mensagens novas"), "success");
  };
  useEffect(() => {
    if (!isTauri) return;
    let off: (() => void) | undefined;
    let alive = true;
    import("@tauri-apps/api/event")
      .then(({ listen }) => listen<number>("mail-changed", (e) => { onMailChanged.current(e.payload).catch(() => {}); }))
      .then((un) => (alive ? (off = un) : un()));
    return () => { alive = false; off?.(); };
  }, []);

  // Menu da bandeja (desktop): "Escrever" e "Sincronizar" chegam como eventos.
  const trayRef = useRef({ write: () => {}, sync: () => {} });
  trayRef.current = { write: writeNew, sync: () => void sync(true) };
  useEffect(() => {
    if (!isTauri) return;
    const offs: (() => void)[] = [];
    let alive = true;
    import("@tauri-apps/api/event").then(async ({ listen }) => {
      for (const [name, run] of [["tray-compose", () => trayRef.current.write()], ["tray-sync", () => trayRef.current.sync()]] as const) {
        const un = await listen(name, run);
        if (alive) offs.push(un); else un();
      }
    });
    return () => { alive = false; offs.forEach((f) => f()); };
  }, []);

  // Busca local a cada tecla: sem debounce, o SQLite responde em < 1 ms.
  useEffect(() => {
    if (!query.trim()) return setResults(null);
    let alive = true;
    api.search(query, folder).then((r) => alive && setResults(r));
    return () => { alive = false; };
  }, [query, folder]);

  // Fio da mensagem aberta: recarrega quando chega mensagem nova nele.
  const threadCount = current?.thread_count;
  useEffect(() => {
    if (selectedId === null || !grouped) return setThread([]);
    let alive = true;
    api.thread(selectedId).then((t) => alive && setThread(t)).catch(() => {});
    return () => { alive = false; };
  }, [selectedId, grouped, threadCount]);
  useEffect(() => { setBodies({}); setExpanded(new Set()); }, [selectedId]);

  // Com a conta removida só resta uma caixa: a visão unificada deixa de existir.
  useEffect(() => { if (folder === "all" && accounts && accounts.length < 2) openFolder("inbox"); });

  function openFolder(role: View) {
    setShowDrafts(false);
    if (role === folder) return;
    setQuery("");
    setResults(null);
    setMessages([]);
    setSelectedId(null);
    setDetail(null);
    setChecked(new Set());
    setReaderOpen(false);
    setFull(false);
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
      const markLocal = (l: MessageSummary[]) => l.map((m) => (m.id === id ? { ...m, seen: true } : m));
      setMessages(markLocal);
      setResults((r) => r && markLocal(r));
      refreshCounts();
      // Abrir a conversa lê o fio todo.
      const rest = (rowOf.get(id)?.members ?? []).filter((m) => m.id !== id && !m.seen);
      if (rest.length) void markSeen(rest, true);
    }
  }

  /** Abre uma mensagem anterior do fio (marca como lida) ou a recolhe. */
  async function toggleThreadMsg(id: number) {
    if (expanded.has(id)) return setExpanded((s) => { const n = new Set(s); n.delete(id); return n; });
    if (!bodies[id]) {
      try {
        const d = await api.open(id);
        if (!d) return;
        setBodies((b) => ({ ...b, [id]: d }));
        const seen = (l: MessageSummary[]) => l.map((m) => (m.id === id ? { ...m, seen: true } : m));
        setMessages(seen);
        setResults((r) => r && seen(r));
        setThread(seen);
        refreshCounts();
      } catch (e) {
        return toast(`Não foi possível abrir a mensagem: ${e}`, "error");
      }
    }
    setExpanded((s) => new Set(s).add(id));
  }

  async function saveEml(id: number) {
    try {
      toast(`Salvo em ${await api.saveEml(id)}`, "success");
    } catch (e) {
      toast(`Não foi possível salvar a mensagem: ${e}`, "error");
    }
  }

  async function saveAttachment(id: number, a: Attachment) {
    const key = `${id}:${a.index}`;
    setSavingAtt(key);
    try {
      const path = await api.saveAttachment(id, a.index, true);
      toast(`Salvo em ${path}`, "success");
    } catch (e) {
      toast(`Não foi possível baixar ${a.name}: ${e}`, "error");
    } finally {
      setSavingAtt((k) => (k === key ? null : k));
    }
  }

  function openComposer(init: ComposeInit) {
    setComposeKey((k) => k + 1);
    setCompose(init);
  }

  /** Assinatura da conta, no padrão "-- " que os leitores de email reconhecem. */
  const signatureFor = (account: string) => {
    const sig = settings[`signature:${account}`]?.trim();
    return sig ? `<div><br></div>${textToHtml(`-- \n${sig}`)}` : "";
  };

  function writeNew() {
    if (!accounts?.length) return setDialog(true);
    openComposer({ title: "Nova mensagem", account: accounts[0].id, reply_to: null, to: "", subject: "", body: "", html: `<div><br></div>${signatureFor(accounts[0].id)}` });
  }

  function openDraft(d: StoredDraft) {
    if (!accounts?.length) return;
    const account = accounts.some((a) => a.id === d.account) ? d.account : accounts[0].id;
    openComposer({ title: d.reply_to !== null ? "Responder" : "Nova mensagem", draftId: d.id, account, reply_to: d.reply_to, to: d.to, cc: d.cc, bcc: d.bcc, subject: d.subject, body: d.body, html: d.html || undefined });
  }

  function openDrafts() {
    setShowDrafts(true);
    setReaderOpen(false);
    refreshDrafts();
  }

  /** Tema e fonte vivem no <html>; "sistema" e "M" ficam sem atributo. */
  useEffect(() => {
    const root = document.documentElement;
    if (settings.theme === "light" || settings.theme === "dark") root.dataset.theme = settings.theme;
    else delete root.dataset.theme;
    if (settings.size === "s" || settings.size === "l") root.dataset.size = settings.size;
    else delete root.dataset.size;
  }, [settings.theme, settings.size]);

  // Tela cheia só existe com uma mensagem aberta e fora das Configurações.
  useEffect(() => { if (full && !current) setFull(false); }, [full, current]);

  /** Muda só na tela (arrasto das colunas); `changeSetting` grava ao soltar. */
  const previewSetting = (key: string, value: string) => setSettings((s) => ({ ...s, [key]: value }));

  function changeSetting(key: string, value: string) {
    setSettings((s) => ({ ...s, [key]: value }));
    api.setSetting(key, value).catch((e) => toast(`Não consegui salvar a configuração: ${e}`, "error"));
  }

  function accountSaved(a: Account) {
    setAccounts((l) => (l ?? []).map((x) => (x.id === a.id ? a : x)));
  }

  /** Sai da pasta aberta se ela (ou a mãe dela) deixou de existir ou mudou de nome. */
  function followFolder(node: FolderNode, renamedTo: string | null) {
    const box = parseBox(folder);
    if (!box || box.account !== node.account) return;
    const under = box.name === node.name || (node.delimiter !== "" && box.name.startsWith(node.name + node.delimiter));
    if (!under) return;
    if (renamedTo === null) openFolder("inbox");
    else setFolder(boxView(box.account, renamedTo + box.name.slice(node.name.length)));
  }

  async function submitFolder(account: string, parent: string | null, name: string) {
    const st = folderDialog;
    if (!st) return;
    if (st.mode === "create") {
      setUserFolders(await api.createFolder(account, parent, name));
      toast("Pasta criada", "success");
      return;
    }
    const { node } = st;
    setUserFolders(await api.renameFolder(node.account, node.name, name));
    const at = node.delimiter ? node.name.lastIndexOf(node.delimiter) : -1;
    followFolder(node, (at >= 0 ? node.name.slice(0, at + 1) : "") + name);
    toast("Pasta renomeada", "success");
  }

  async function removeFolder(node: FolderNode) {
    setFolderBusy(true);
    try {
      setUserFolders(await api.deleteFolder(node.account, node.name));
      followFolder(node, null);
      toast("Pasta excluída", "success");
    } catch (e) {
      toast(`Não foi possível excluir a pasta: ${e}`, "error");
    } finally {
      setFolderBusy(false);
      setFolderDelete(null);
    }
  }

  /** Pasta vazia some direto; com mensagens (contando as subpastas) pede confirmação. */
  async function askDeleteFolder(node: FolderNode) {
    try {
      const count = await api.countFolder(node.account, node.name);
      if (count === 0) await removeFolder(node);
      else setFolderDelete({ node, count });
    } catch (e) {
      toast(`Não consegui consultar a pasta: ${e}`, "error");
    }
  }

  /** Aplica (ou tira) a categoria na hora; na visão de uma categoria, a mensagem sai da lista se mudou de categoria. */
  async function applyCategory(ms: MessageSummary[], id: string | null) {
    const ids = new Set(ms.map((m) => m.id));
    const set = (l: MessageSummary[]) => l.map((m) => (ids.has(m.id) ? { ...m, category: id } : m));
    setMessages(set);
    setResults((r) => r && set(r));
    try {
      await api.setCategory([...ids], id);
      const inCat = parseCat(folder);
      if (inCat && id !== inCat) takeOutMany(ms);
    } catch (e) {
      toast(`Não consegui aplicar a categoria: ${e}`, "error");
      reload(messages.length).catch(() => {});
    }
  }

  function endDrag() {
    dragging.current = [];
    setDragIds(new Set());
    setDropKey(null);
  }

  /** Alvo de soltar mensagens arrastadas da lista; a pasta aberta não é alvo. */
  function dropProps(key: string, view: View, dest: Dest): Record<string, unknown> {
    if (view === folder || showDrafts) return {};
    return {
      "data-drop": dropKey === key || undefined,
      onDragOver: (e: React.DragEvent) => {
        if (!dragging.current.length) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        setDropKey(key);
      },
      onDragLeave: (e: React.DragEvent) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropKey((k) => (k === key ? null : k));
      },
      onDrop: (e: React.DragEvent) => {
        e.preventDefault();
        const ms = dragging.current;
        endDrag();
        if (!ms.length) return;
        if (typeof dest === "string") {
          const todo = ms.filter((m) => m.role !== dest);
          if (todo.length) moveOut(todo, dest);
          return;
        }
        const same = ms.filter((m) => m.account === dest.account);
        const other = ms.length - same.length;
        if (other) toast(`${plural(other, "mensagem é", "mensagens são")} de outra conta e não ${other === 1 ? "foi movida" : "foram movidas"}`, "info");
        if (same.length) moveOut(same, dest);
      },
    };
  }

  async function removeAccount(id: string) {
    await api.removeAccount(id);
    setAccounts((l) => (l ?? []).filter((a) => a.id !== id));
    setSettings((s) => Object.fromEntries(Object.entries(s).filter(([k]) => k !== `signature:${id}`)));
    setAccountFilter((f) => (f === id ? null : f));
    setSelectedId(null);
    setDetail(null);
    setReaderOpen(false);
    api.folders().then(setFolderList).catch(() => {});
    api.userFolders().then(setUserFolders).catch(() => {});
    refreshDrafts();
    reload().catch(() => {});
    toast("Conta removida", "success");
  }

  /** Responde pelo alias para o qual a mensagem foi enviada (se for de um da conta). */
  const replyFrom = (d: MessageDetail, account: string) => aliasFor(aliasLines(settings[`aliases:${account}`]), d.to_addrs, d.cc_addrs);

  function reply() {
    if (!detail || !accounts?.length) return;
    const subject = /^re:/i.test(detail.subject) ? detail.subject : `Re: ${detail.subject}`;
    const account = accounts.some((a) => a.id === detail.account) ? detail.account : accounts[0].id;
    openComposer({ title: "Responder", account, reply_to: detail.id, to: detail.reply_addr, subject, body: "", html: signatureFor(account) + replyHtml(detail), from: replyFrom(detail, account) });
  }

  /** Remetente em Para; os demais destinatários da original em Cc, sem as minhas contas e sem repetir. */
  function replyAll() {
    if (!detail || !accounts?.length) return;
    const own = new Set([...accounts.map((a) => a.username.toLowerCase()), ...accounts.flatMap((a) => aliasLines(settings[`aliases:${a.id}`]).map(addrOf))]);
    const split = (v: string) => v.split(",").map((x) => x.trim()).filter(Boolean);
    const seen = new Set([detail.reply_addr.toLowerCase()]);
    const others = [...split(detail.to_addrs), ...split(detail.cc_addrs)].filter((a) => {
      const k = a.toLowerCase();
      if (own.has(k) || seen.has(k)) return false;
      seen.add(k);
      return true;
    });
    const subject = /^re:/i.test(detail.subject) ? detail.subject : `Re: ${detail.subject}`;
    const account = accounts.some((a) => a.id === detail.account) ? detail.account : accounts[0].id;
    openComposer({
      title: "Responder a todos", account, reply_to: detail.id,
      to: detail.reply_addr, cc: others.join(", "), subject, body: "", html: replyHtml(detail), from: replyFrom(detail, account),
    });
  }

  async function forward() {
    if (!detail || !accounts?.length) return;
    const subject = /^(fwd?|enc):/i.test(detail.subject) ? detail.subject : `Fwd: ${detail.subject}`;
    const account = accounts.some((a) => a.id === detail.account) ? detail.account : accounts[0].id;
    let attachments: OutAttachment[] = [];
    if (detail.attachments.length) {
      try {
        attachments = await api.forwardAttachments(detail.id);
      } catch (e) {
        toast(`Anexos não incluídos no encaminhamento: ${e}`, "error");
      }
    }
    openComposer({ title: "Encaminhar", account, reply_to: null, to: "", subject, body: "", html: forwardHtml(detail), attachments });
  }

  // O painel fecha na hora; o envio espera o "Desfazer" expirar. Se falhar, o rascunho volta intacto.
  function send(d: ComposeInit) {
    setCompose(null);
    toast("Enviando…", "sending", {
      action: { label: "Desfazer", onClick: () => openComposer({ ...d, restored: true }) },
      duration: (Number(settings.undoSend) || 4) * 1000,
      onClose: async (reason) => {
        if (reason === "action") return;
        const { title: _t, restored: _r, draftId, ...draft } = d;
        try {
          const r = await api.send(draft);
          // Enviada: o rascunho cumpriu o papel. Se o envio falhar ou for desfeito, ele continua salvo.
          if (draftId) api.deleteDraft(draftId).then(refreshDrafts).catch(() => {});
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
      } else if (remote && serverTotal !== null && serverTotal > keep) {
        const next = keep + KEEP_STEP;
        setKeep(next);
        await syncFolder(folder as Role, next);
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
    const ridx = rows.findIndex((r) => r.head.id === m.id);
    const next = (rows[ridx + 1] ?? rows[ridx - 1])?.head;
    const drop = (l: MessageSummary[]) => l.filter((x) => x.id !== m.id);
    setMessages(drop);
    setResults((r) => r && drop(r));
    // Deslizar na lista não abre mensagem: no celular o leitor cobriria a lista e o próximo gesto cairia nele.
    const quiet = swiping.current;
    if (quiet) {
      if (selectedId === m.id) { setSelectedId(null); setDetail(null); setReaderOpen(false); }
    } else if (next) select(next.id, true);
    else { setSelectedId(null); setDetail(null); setReaderOpen(false); }
    return () => {
      if (folderRef.current !== from) return;
      const put = (l: MessageSummary[]) => (l.some((x) => x.id === m.id) ? l : [...l.slice(0, idx), m, ...l.slice(idx)]);
      setMessages(put);
      setResults((r) => r && put(r));
      if (!quiet) select(m.id, true);
    };
  }

  /** Tira várias da lista de uma vez (sem escolher vizinha) e devolve a função que as traz de volta. */
  function takeOutMany(ms: MessageSummary[]) {
    if (ms.length === 1) return takeOut(ms[0]);
    const from = folder;
    const gone = new Set(ms.map((m) => m.id));
    const drop = (l: MessageSummary[]) => l.filter((x) => !gone.has(x.id));
    setMessages(drop);
    setResults((r) => r && drop(r));
    setChecked(new Set());
    if (selectedId !== null && gone.has(selectedId)) { setSelectedId(null); setDetail(null); setReaderOpen(false); }
    return () => {
      if (folderRef.current === from) reload(messages.length).catch(() => {});
    };
  }

  /** Sai da lista na hora; o servidor só recebe quando o "Desfazer" expira. */
  function moveOut(ms: MessageSummary[], to: Dest) {
    const ids = ms.map((m) => m.id);
    const undo = takeOutMany(ms);
    ids.forEach((id) => pendingMove.current.add(id));
    const restore = () => { ids.forEach((id) => pendingMove.current.delete(id)); undo(); };
    const text = typeof to !== "string" ? `Movida para ${to.label}`
      : to === "trash" ? "Movida para a Lixeira" : to === "inbox" ? "Movida para a Caixa de entrada" : to === "archive" ? "Arquivada" : `Movida para ${FOLDERS[to].label}`;
    toast(ms.length === 1 ? text : `${plural(ms.length, "mensagem movida", "mensagens movidas")}`, "info", {
      action: { label: "Desfazer", onClick: restore },
      duration: UNDO_MS,
      onClose: async (reason) => {
        if (reason === "action") return;
        try {
          if (typeof to === "string") await api.moveMany(ids, to);
          else await api.moveToFolder(ids, to.account, to.name);
          ids.forEach((id) => pendingMove.current.delete(id));
          refreshCounts();
        } catch (e) {
          restore();
          toast(`Não foi possível mover: ${e}`, "error");
        }
      },
    });
  }

  /** Gestos de deslizar a linha (toque): configuráveis em Configurações. */
  const swipeAction = (dir: SwipeDir): SwipeAction => {
    const v = dir === "right" ? settings.swipeRight : settings.swipeLeft;
    return v === "archive" || v === "trash" || v === "read" || v === "none" ? v : dir === "right" ? "archive" : "trash";
  };
  function swipeRow(row: Row, dir: SwipeDir) {
    const what = swipeAction(dir);
    swiping.current = true;
    try {
      if (what === "archive") archive(row.members);
      else if (what === "trash") remove(row.members);
      else if (what === "read") markSeen(row.members, row.members.some((x) => !x.seen));
    } finally {
      swiping.current = false;
    }
  }

  /** Agrupa por destino: na pasta "Com estrela" cada mensagem pode ter um destino diferente. */
  function moveEach(ms: MessageSummary[], dest: (m: MessageSummary) => Role) {
    const groups = new Map<Role, MessageSummary[]>();
    ms.forEach((m) => groups.set(dest(m), [...(groups.get(dest(m)) ?? []), m]));
    groups.forEach((list, to) => moveOut(list, to));
  }

  function archive(ms: MessageSummary[]) {
    moveEach(ms.filter(canArchive), archiveTo);
  }

  function remove(ms: MessageSummary[]) {
    const ok = ms.filter(canTrash);
    const forever = ok.filter((m) => m.role === "trash");
    if (forever.length) setConfirmDelete(forever);
    moveEach(ok.filter((m) => m.role !== "trash"), () => "trash");
  }

  async function deleteForever() {
    if (!confirmDelete || deleting) return;
    const list = confirmDelete;
    setDeleting(true);
    try {
      await api.deleteForeverMany(list.map((m) => m.id));
      takeOutMany(list);
      toast(list.length === 1 ? "Excluída de vez" : `${list.length} excluídas de vez`, "success");
      refreshCounts();
    } catch (e) {
      toast(`Não foi possível excluir: ${e}`, "error");
    } finally {
      setDeleting(false);
      setConfirmDelete(null);
    }
  }

  /** Lida/não lida: muda na lista na hora e desfaz se o servidor recusar. */
  async function markSeen(ms: MessageSummary[], seen: boolean) {
    const todo = ms.filter((m) => m.seen !== seen);
    if (!todo.length) return;
    const ids = new Set(todo.map((m) => m.id));
    const set = (v: boolean) => (l: MessageSummary[]) => l.map((m) => (ids.has(m.id) ? { ...m, seen: v } : m));
    setMessages(set(seen));
    setResults((r) => r && set(seen)(r));
    bumpCounts(todo, seen ? -1 : 1);
    try {
      await api.setSeen([...ids], seen);
    } catch (e) {
      setMessages(set(!seen));
      setResults((r) => r && set(!seen)(r));
      toast(`Não foi possível marcar: ${e}`, "error");
    }
    refreshCounts();
  }

  /** Estrela: se todas já têm, tira; senão põe em todas. Só o clique do mouse anima. */
  async function toggleStar(ms: MessageSummary[], keyboard = false) {
    if (!ms.length) return;
    const on = !ms.every((m) => m.flagged);
    const ids = new Set(ms.filter((m) => m.flagged !== on).map((m) => m.id));
    const set = (v: boolean) => (l: MessageSummary[]) => l.map((m) => (ids.has(m.id) ? { ...m, flagged: v } : m));
    setMessages(set(on));
    setResults((r) => r && set(on)(r));
    if (!keyboard && ms.length === 1) {
      setPopId(ms[0].id);
      setTimeout(() => setPopId(null), 400);
    }
    try {
      await api.setFlagged([...ids], on);
    } catch (e) {
      setMessages(set(!on));
      setResults((r) => r && set(!on)(r));
      toast(`Não foi possível marcar com estrela: ${e}`, "error");
    }
  }

  function toggleCheck(m: MessageSummary, range: boolean) {
    const anchorId = anchor.current ?? selectedId;
    const from = rows.findIndex((r) => r.members.some((x) => x.id === anchorId));
    const to = rows.findIndex((r) => r.head.id === m.id);
    setChecked((c) => {
      const next = new Set(c);
      if (range && from >= 0) {
        rows.slice(Math.min(from, to), Math.max(from, to) + 1).forEach((r) => next.add(r.head.id));
      } else if (!next.delete(m.id)) {
        next.add(m.id);
      }
      return next;
    });
    anchor.current = m.id;
  }

  function onRowClick(e: React.MouseEvent, m: MessageSummary) {
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      e.preventDefault();
      toggleCheck(m, e.shiftKey);
    } else {
      select(m.id);
      if (pane === "off") setFull(true);
    }
  }

  // No desktop abre o diálogo com as novidades; no Android (APK) só um aviso discreto com "Atualizar".
  const notified = useRef(""); // o aviso do APK é um toast: não repete a mesma versão (nem no StrictMode)
  const showUpdate = useCallback((u: Update) => {
    if (!u.apkUrl) return setUpdate(u);
    toast(`Nova versão v${u.version} disponível`, "info", {
      duration: 20000,
      action: {
        label: "Atualizar",
        onClick: () => u.install(() => {}).catch((e) => toast(`Não consegui abrir o download: ${e}`, "error")),
      },
    });
  }, []);

  // Atualização: procura ao abrir e a cada 6 h, em silêncio; só avisa se houver versão nova não adiada.
  useEffect(() => {
    const run = () => {
      if (!autoCheck()) return;
      checkUpdate()
        .then((u) => {
          if (!u || u.version === skipped() || (u.apkUrl && u.version === notified.current)) return;
          if (u.apkUrl) notified.current = u.version;
          showUpdate(u);
        })
        .catch((e) => console.warn("procurar atualização falhou:", e));
    };
    run();
    const timer = setInterval(run, CHECK_EVERY_MS);
    return () => clearInterval(timer);
  }, [showUpdate]);

  // Atalhos: j/k ou setas navegam, / busca, r responde, a responde a todos, f encaminha, c escreve, e arquiva, # exclui,
  // s estrela, u marca não lida. Com mensagens marcadas (checkbox), e/#/s/u valem para todas. Esc limpa a busca, a seleção ou volta.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (dialog || compose || settingsOpen || update || folderDialog) return;
      if (folderDelete) {
        if (e.key === "Escape" && !folderBusy) setFolderDelete(null);
        return;
      }
      if (confirmDelete) {
        if (e.key === "Escape" && !deleting) setConfirmDelete(null);
        return;
      }
      if (showDrafts && !(e.key === "c" || e.key === "Escape")) return;
      const t = e.target;
      const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
      if (e.key === "Escape") {
        if (full && !typing) setFull(false);
        else if (typing) { setQuery(""); searchRef.current?.blur(); }
        else if (checked.size) setChecked(new Set());
        else setReaderOpen(false);
        return;
      }
      if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
      const idx = rows.findIndex((r) => r.members.some((m) => m.id === selectedId));
      const acts = checkedList.length ? checkedList : current ? withThread([current]) : [];
      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        const next = rows[Math.min(idx + 1, rows.length - 1)]?.head;
        if (next) select(next.id, true);
      } else if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        const prev = rows[Math.max(idx - 1, 0)]?.head;
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
      } else if (e.key === "a") {
        e.preventDefault();
        replyAll();
      } else if (e.key === "f") {
        e.preventDefault();
        forward();
      } else if (e.key === "e") {
        archive(acts);
      } else if (e.key === "#") {
        remove(acts);
      } else if (e.key === "s") {
        toggleStar(acts, true);
      } else if (e.key === "u") {
        markSeen(acts, false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>("[aria-selected=true]")?.scrollIntoView({ block: "nearest" });
  }, [selectedId]);

  const empty = accounts !== null && accounts.length === 0;
  /** Conversa com mais de uma mensagem aberta no painel (o fio carrega logo após a seleção). */
  const threaded = grouped && !!detail && thread.length > 1 && thread.some((t) => t.id === detail.id);
  const actionsBar = (
      <div className="actions">
        <button className="btn" onClick={reply}>
          <ArrowBendUpLeft size={15} weight="bold" /> Responder <kbd>r</kbd>
        </button>
        <button className="btn icon-only" onClick={replyAll} aria-label="Responder a todos" title="Responder a todos (a)">
          <ArrowBendDoubleUpLeft size={15} weight="bold" />
        </button>
        <button className="btn icon-only" onClick={forward} aria-label="Encaminhar" title="Encaminhar (f)">
          <ArrowBendUpRight size={15} weight="bold" />
        </button>
        {current && canArchive(current) && (
          <button className="btn" onClick={() => archive(withThread([current]))}>
            {archiveTo(current) === "inbox"
              ? <><Tray size={15} weight="bold" /> Mover para a entrada</>
              : <><Archive size={15} weight="bold" /> Arquivar</>} <kbd>e</kbd>
          </button>
        )}
        <button className="btn" onClick={() => current && remove(withThread([current]))} disabled={!current || !canTrash(current)}
          title={current && !canTrash(current) ? "Esta conta não tem Lixeira" : undefined}>
          <Trash size={15} weight="bold" /> {current?.role === "trash" ? "Excluir de vez" : "Excluir"} <kbd>#</kbd>
        </button>
        {current && (
          <>
            {!ANDROID && (
              <button className="btn icon-only" onClick={() => window.print()}
                aria-label="Imprimir ou salvar como PDF" title="Imprimir ou salvar como PDF">
                <Printer size={15} weight="bold" />
              </button>
            )}
            <button className="btn icon-only" onClick={() => void saveEml(current.id)}
              aria-label="Salvar como .eml" title="Salvar como .eml">
              <FloppyDisk size={15} weight="bold" />
            </button>
            <button className="btn icon-only star-btn" data-on={current.flagged || undefined} data-pop={popId === current.id || undefined}
              onClick={() => toggleStar(withThread([current]))} aria-pressed={current.flagged}
              aria-label={current.flagged ? "Tirar a estrela" : "Marcar com estrela"} title="Estrela (s)">
              <Star size={15} weight={current.flagged ? "fill" : "bold"} />
            </button>
            <button className="btn icon-only" onClick={() => markSeen(withThread([current]), false)}
              aria-label="Marcar como não lida" title="Marcar como não lida (u)">
              <EnvelopeSimple size={15} weight="bold" />
            </button>
            <CategoryPicker className="btn icon-only" categories={categories} current={current.category ?? null} onPick={(id) => applyCategory(withThread([current]), id)} />
          </>
        )}
      </div>
  );

  /** Cabeçalho, anexos e corpo de uma mensagem. Em conversa, as anteriores trazem "Recolher" no lugar de "Expandir". */
  function renderMessage(d: MessageDetail, fold: boolean) {
    return (
      <>
        <div className="meta">
              <span className="avatar large" aria-hidden>{(d.from_name || d.from_addr)[0]?.toUpperCase()}</span>
              <div>
                <div className="meta-from">{d.from_name || d.from_addr}</div>
                <div className="meta-addr">{d.from_addr}</div>
              </div>
              <time className="meta-date">{fmtLong.format(new Date(d.date * 1000))}</time>
              {fold ? (
                <button type="button" className="icon-btn expand-btn" onClick={() => void toggleThreadMsg(d.id)}
                  aria-label="Recolher mensagem" title="Recolher">
                  <CaretUp size={15} weight="bold" />
                </button>
              ) : (
                <button type="button" className="icon-btn expand-btn" onClick={() => setFull(!full)}
                  aria-label={full ? "Sair da tela cheia" : "Expandir mensagem"} title={full ? "Voltar (Esc)" : "Expandir (clique duplo na lista)"}>
                  {full ? <ArrowsIn size={15} weight="bold" /> : <ArrowsOut size={15} weight="bold" />}
                </button>
              )}
            </div>
            {!threaded && d.id === selectedId && actionsBar}
            {d.attachments.length > 0 && (
              <ul className="attachments" aria-label="Anexos">
                {d.attachments.map((a) => (
                  <li key={a.index}>
                    <button type="button" className="chip attachment" disabled={savingAtt === `${d.id}:${a.index}`}
                      title={`Salvar e abrir ${a.name}`} onClick={() => void saveAttachment(d.id, a)}>
                      <Paperclip size={14} weight="bold" aria-hidden />
                      <span className="chip-name">{a.name}</span>
                      <span className="chip-size">{fmtSize(a.size)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {d.body_html ? (
              <>
                {!imagesOk.has(d.id) && hasRemoteImages(d.body_html) && (
                  <div className="img-banner" role="status">
                    <span>Imagens remotas bloqueadas para proteger sua privacidade.</span>
                    <button type="button" className="btn" onClick={() => setImagesOk((s) => new Set(s).add(d.id))}>
                      Mostrar imagens
                    </button>
                  </div>
                )}
                <HtmlBody html={d.body_html} remote={imagesOk.has(d.id)} />
              </>
            ) : (
              <div className="body">{d.body_text}</div>
            )}
      </>
    );
  }

  return (
    <div ref={appRef} className="app" data-reader={readerOpen || undefined} data-pane={pane === "right" ? undefined : pane}
      data-full={full || undefined} data-page={settingsOpen ? "settings" : undefined} data-ready={prefsReady || undefined}
      data-density={settings.density === "compact" ? "compact" : undefined} data-preview={settings.preview === "0" ? "0" : undefined}
      style={layoutVars}>
      <aside className="rail">
        <div className="brand">Posta</div>
        <button className="btn primary compose-btn" onClick={writeNew} aria-label="Escrever">
          <NotePencil size={16} weight="bold" /> <span>Escrever</span>
        </button>
        <nav>
          {navFolders.map((r) => {
            const Icon = FOLDERS[r].icon;
            return (
              <button key={r} className="nav-item" title={FOLDERS[r].label} onClick={() => openFolder(r)}
                aria-current={r === folder && !showDrafts ? "page" : undefined}
                {...(r === "starred" || r === "all" ? {} : dropProps(r, r, r))}>
                <Icon size={16} weight="bold" />
                <span>{FOLDERS[r].label}</span>
                {!!unreadOf(r) && <span className="count">{unreadOf(r)}</span>}
              </button>
            );
          })}
          <button className="nav-item" title="Rascunhos" onClick={openDrafts} aria-current={showDrafts ? "page" : undefined}>
            <FileText size={16} weight="bold" />
            <span>Rascunhos</span>
            {drafts.length > 0 && <span className="count plain">{drafts.length}</span>}
          </button>
        </nav>
        <FolderTree nodes={tree} accounts={accounts ?? []} byAccount={(accounts?.length ?? 0) > 1 && !accountFilter}
          categories={categories} folder={folder} inList={!showDrafts} open={openFolder}
          drop={(key, view) => {
            const box = parseBox(view);
            return box ? dropProps(key, view, { account: box.account, name: box.name, label: viewInfo(view, userFolders, categories).label }) : {};
          }}
          onNew={(parent) => setFolderDialog({ mode: "create", account: parent?.account ?? accountFilter ?? undefined, parent: parent?.name })}
          onRename={(node) => setFolderDialog({ mode: "rename", node })} onDelete={askDeleteFolder} />
        <div className="rail-section">
          <div className="rail-label">Contas</div>
          {accounts && accounts.length > 1 && (
            <button type="button" className="account account-btn" title="Todas as caixas juntas" aria-pressed={!accountFilter}
              onClick={() => setAccountFilter(null)}>
              <span className="avatar" aria-hidden>*</span>
              <span className="account-name">Todas as contas</span>
            </button>
          )}
          {accounts?.map((a) => accounts.length > 1 ? (
            <button key={a.id} type="button" className="account account-btn" title={a.id} aria-pressed={accountFilter === a.id}
              onClick={() => { setAccountFilter(accountFilter === a.id ? null : a.id); if (folder === "all") openFolder("inbox"); }}>
              <span className="avatar" aria-hidden>{a.username[0]?.toUpperCase()}</span>
              <span className="account-name">{a.username}</span>
            </button>
          ) : (
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
          <span><kbd>a</kbd> responder a todos</span>
          <span><kbd>f</kbd> encaminhar</span>
          <span><kbd>c</kbd> escrever</span>
          <span><kbd>e</kbd> arquivar</span>
          <span><kbd>#</kbd> excluir</span>
          <span><kbd>s</kbd> estrela</span>
          <span><kbd>u</kbd> não lida</span>
        </div>
        <button className="btn ghost small settings-btn" onClick={() => setSettingsOpen(true)} aria-label="Configurações">
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
          <button className="icon-btn mobile-only" onClick={() => setSettingsOpen(true)} aria-label="Configurações">
            <GearSix size={16} weight="bold" />
          </button>
        </div>
        <div className="folder-tabs mobile-only" role="group" aria-label="Pastas">
          {navFolders.map((r) => {
            const Icon = FOLDERS[r].icon;
            return (
              <button key={r} className="btn ghost small" onClick={() => openFolder(r)}
                aria-current={r === folder && !showDrafts ? "page" : undefined}>
                <Icon size={14} weight="bold" /> {FOLDERS[r].label}
              </button>
            );
          })}
          {[...tree.map((n) => boxView(n.account, n.name)), ...categories.map((c) => catView(c.id))].map((v) => {
            const { label, icon: Icon } = viewInfo(v, userFolders, categories);
            return (
              <button key={v} className="btn ghost small" onClick={() => openFolder(v)} aria-current={v === folder && !showDrafts ? "page" : undefined}>
                <Icon size={14} weight="bold" /> {label}
              </button>
            );
          })}
          <button className="btn ghost small" onClick={openDrafts} aria-current={showDrafts ? "page" : undefined}>
            <FileText size={14} weight="bold" /> Rascunhos
          </button>
        </div>
        {checkedList.length > 0 && (
          <div className="bulk" role="toolbar" aria-label="Ações em lote">
            <button className="icon-btn" onClick={() => setChecked(new Set())} aria-label="Limpar seleção" title="Limpar seleção (Esc)">
              <X size={15} weight="bold" />
            </button>
            <span className="bulk-count">{checkedRows.length} {checkedRows.length === 1 ? "selecionada" : "selecionadas"}</span>
            {checkedRows.length < rows.length && (
              <button className="btn ghost small" onClick={() => setChecked(new Set(rows.map((r) => r.head.id)))}>Todas</button>
            )}
            <span className="bulk-actions">
              <button className="icon-btn" onClick={() => archive(checkedList)} disabled={!checkedList.some(canArchive)}
                aria-label="Arquivar" title="Arquivar (e)"><Archive size={16} weight="bold" /></button>
              <button className="icon-btn" onClick={() => remove(checkedList)} disabled={!checkedList.some(canTrash)}
                aria-label="Excluir" title="Excluir (#)"><Trash size={16} weight="bold" /></button>
              <button className="icon-btn" onClick={() => markSeen(checkedList, true)}
                aria-label="Marcar como lida" title="Marcar como lida"><EnvelopeSimpleOpen size={16} weight="bold" /></button>
              <button className="icon-btn" onClick={() => markSeen(checkedList, false)}
                aria-label="Marcar como não lida" title="Marcar como não lida (u)"><EnvelopeSimple size={16} weight="bold" /></button>
              <button className="icon-btn" onClick={() => toggleStar(checkedList)}
                aria-label="Alternar estrela" title="Alternar estrela (s)"><Star size={16} weight="bold" /></button>
              <CategoryPicker className="icon-btn" categories={categories} onPick={(id) => applyCategory(checkedList, id)}
                current={checkedList.every((m) => m.category === checkedList[0].category) ? checkedList[0].category ?? null : undefined} />
            </span>
          </div>
        )}
        {results && !showDrafts && (
          <div className="result-meta">{plural(results.length, "resultado", "resultados")} para “{query.trim()}”</div>
        )}

        {showDrafts ? (
          draftsShown.length === 0 ? (
            <div className="empty quiet"><p>Nenhum rascunho</p></div>
          ) : (
            <ul className="list" role="listbox" aria-label="Rascunhos">
              {draftsShown.map((d) => (
                <li key={d.id} role="option" aria-selected={false} className="row draft-row" onClick={() => openDraft(d)}>
                  <span className="dot" aria-hidden />
                  <span className="from">{d.to.trim() || "(sem destinatário)"}</span>
                  <time className="date" dateTime={new Date(d.updated * 1000).toISOString()}>{shortDate(d.updated)}</time>
                  <span className="subject">{d.subject || "(sem assunto)"}</span>
                  <span className="snippet">{d.body.trim().replace(/\s+/g, " ").slice(0, 200)}</span>
                </li>
              ))}
            </ul>
          )
        ) : empty ? (
          <div className="empty">
            <EnvelopeSimple size={28} weight="duotone" />
            <h2>Conecte sua primeira conta</h2>
            <p>Gmail, Outlook ou qualquer servidor IMAP. Os emails ficam salvos neste aparelho para abrir na hora.</p>
            <button className="btn primary" onClick={() => setDialog(true)}>Adicionar conta</button>
          </div>
        ) : results && results.length === 0 ? (
          <div className="empty quiet"><p>Nada encontrado.</p></div>
        ) : !results && visible.length === 0 ? (
          <div className="empty quiet"><p>{loadingFolder ? "Carregando…" : info.empty}</p></div>
        ) : (
          <ul ref={listRef} className="list" role="listbox" aria-label="Mensagens" data-intro={intro || undefined}
            data-selecting={checkedList.length > 0 || undefined} aria-multiselectable
            onScroll={(e) => {
              const el = e.currentTarget;
              if (el.scrollHeight - el.scrollTop - el.clientHeight < 400) loadMore();
            }}>
            {rows.map((row, i) => {
              const m = row.head;
              const unread = row.members.some((x) => !x.seen);
              const starred = row.members.some((x) => x.flagged);
              const n = grouped ? Math.max(row.members.length, m.thread_count) : 1;
              const open = row.members.some((x) => x.id === selectedId);
              return (
                <li key={m.id} role="option" aria-selected={open}
                  className="row" data-unread={unread || undefined} data-fresh={freshIds.has(m.id) || undefined}
                  data-checked={checked.has(m.id) || undefined} data-dragging={row.members.some((x) => dragIds.has(x.id)) || undefined}
                  draggable
                  onDragStart={(e) => {
                    const ms = checked.has(m.id) ? checkedList : row.members;
                    dragging.current = ms;
                    e.dataTransfer.effectAllowed = "move";
                    e.dataTransfer.setData("text/plain", ms.map((x) => x.subject).join("\n"));
                    setDragIds(new Set(ms.map((x) => x.id)));
                  }}
                  onDragEnd={endDrag}
                  style={{ "--i": Math.min(i, 12) } as React.CSSProperties}
                  {...swipeProps(swipeAction, (dir) => swipeRow(row, dir))}
                  onClick={(e) => { if (!justSwiped()) onRowClick(e, m); }} onDoubleClick={(e) => { if (!(e.target as Element).closest("button")) setFull(true); }}>
                  <span className="dot" aria-hidden />
                  <button type="button" className="check" role="checkbox" aria-checked={checked.has(m.id)}
                    aria-label={`Selecionar ${n > 1 ? "conversa" : "mensagem"} de ${m.from_name || m.from_addr}`}
                    onClick={(e) => { e.stopPropagation(); toggleCheck(m, e.shiftKey); }}>
                    <Check size={11} weight="bold" />
                  </button>
                  <span className="from has-meta">
                    <span className="from-name">{m.from_name || m.from_addr}</span>
                    {n > 1 && <span className="thread-n" title={`${n} mensagens na conversa`}>{n}</span>}
                  </span>
                  <time className="date" dateTime={new Date(m.date * 1000).toISOString()}>{shortDate(m.date)}</time>
                  <button type="button" className="star" data-on={starred || undefined} data-pop={popId === m.id || undefined}
                    aria-pressed={starred} aria-label={starred ? "Tirar a estrela" : "Marcar com estrela"}
                    onClick={(e) => { e.stopPropagation(); toggleStar(row.members); }}>
                    <Star size={15} weight={starred ? "fill" : "bold"} />
                  </button>
                  <span className="subject">
                    {folder === "all" && <span className="acct" title={m.account}>{acctLabel(m.account)}</span>}
                    {categories.filter((c) => c.id === m.category).map((c) => <CategoryChip key={c.id} category={c} />)}
                    {m.subject || "(sem assunto)"}
                  </span>
                  <span className="snippet">{m.snippet}</span>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {!full && !settingsOpen && (
        <>
          <Resizer axis="x" edge="rail" label="Largura das pastas" value={railW} min={RAIL.min} max={() => RAIL.max}
            onChange={(v) => previewSetting("w:rail", String(v))} onCommit={(v) => changeSetting("w:rail", String(v))}
            onReset={() => changeSetting("w:rail", "")} />
          {pane === "right" && (
            <Resizer axis="x" edge="list" label="Largura da lista" value={listW} min={LIST.min} max={listMax}
              onChange={(v) => previewSetting("w:list", String(v))} onCommit={(v) => changeSetting("w:list", String(v))}
              onReset={() => changeSetting("w:list", "")} />
          )}
          {pane === "bottom" && (
            <Resizer axis="y" edge="list" label="Altura da lista" value={listH} min={LIST_H.min} max={listHMax}
              onChange={(v) => previewSetting("h:list", String(v))} onCommit={(v) => changeSetting("h:list", String(v))}
              onReset={() => changeSetting("h:list", "")} />
          )}
        </>
      )}

      <main className="reader">
        {detail && detail.id === selectedId ? (
          <article key={detail.id} className="message" data-instant={viaKeyboard || undefined}>
            <button className="btn ghost small back" onClick={() => (full ? setFull(false) : setReaderOpen(false))}>
              <ArrowLeft size={14} weight="bold" /> {full ? "Voltar" : info.label}
            </button>
            <h1>{detail.subject || "(sem assunto)"}</h1>
            {threaded && actionsBar}
            {threaded ? (
              <div className="thread">
                {thread.map((t) => {
                  const d = t.id === detail.id ? detail : bodies[t.id];
                  if (d && (t.id === detail.id || expanded.has(t.id))) {
                    return <section key={t.id} className="thread-msg" data-current={t.id === detail.id || undefined}>{renderMessage(d, t.id !== detail.id)}</section>;
                  }
                  return (
                    <button key={t.id} type="button" className="thread-fold" data-unread={!t.seen || undefined} aria-expanded="false"
                      onClick={() => void toggleThreadMsg(t.id)}>
                      <span className="avatar" aria-hidden>{(t.from_name || t.from_addr)[0]?.toUpperCase()}</span>
                      <span className="fold-from">{t.from_name || t.from_addr}</span>
                      <span className="fold-snip">{t.snippet}</span>
                      <time className="fold-date">{shortDate(t.date)}</time>
                    </button>
                  );
                })}
              </div>
            ) : renderMessage(detail, false)}
          </article>
        ) : (
          <div className="reader-empty">
            <p>{messages.length ? "Selecione uma mensagem" : ""}</p>
          </div>
        )}
        {confirmDelete && (
          <div className="confirm scrim" onMouseDown={(e) => { if (e.target === e.currentTarget && !deleting) setConfirmDelete(null); }}>
            <div className="confirm-card" role="alertdialog" aria-labelledby="delete-title">
              <p id="delete-title">{confirmDelete.length === 1 ? "Excluir de vez?" : `Excluir ${confirmDelete.length} mensagens de vez?`}</p>
              <span>Não dá para desfazer.</span>
              <div className="confirm-actions">
                <button type="button" className="btn" onClick={() => setConfirmDelete(null)} disabled={deleting} autoFocus>Cancelar</button>
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
            if (settings.notify !== "0") api.requestNotifyPermission().catch(() => {});
            toast(`Conta ${a.username} conectada`, "success");
          }}
        />
      )}
      {compose && accounts && (
        <Composer key={composeKey} init={compose} accounts={accounts} settings={settings} onSend={send} onDiscard={() => setCompose(null)} />
      )}
      {settingsOpen && (
        <SettingsPage accounts={accounts ?? []} settings={settings} onSetting={changeSetting} onRemoveAccount={removeAccount}
          onAddAccount={() => setDialog(true)} onAccountSaved={accountSaved} onClose={() => setSettingsOpen(false)} onUpdate={showUpdate} />
      )}
      {folderDialog && (
        <FolderDialog state={folderDialog} accounts={accounts ?? []} folders={userFolders} onSubmit={submitFolder} onClose={() => setFolderDialog(null)} />
      )}
      {folderDelete && (
        <div className="confirm scrim" onMouseDown={(e) => { if (e.target === e.currentTarget && !folderBusy) setFolderDelete(null); }}>
          <div className="confirm-card" role="alertdialog" aria-labelledby="folder-delete-title">
            <p id="folder-delete-title">Excluir a pasta “{folderDelete.node.leaf}”?</p>
            <span>{plural(folderDelete.count, "mensagem", "mensagens")} (incluindo subpastas) serão excluídas do servidor junto com a pasta.</span>
            <div className="confirm-actions">
              <button type="button" className="btn" onClick={() => setFolderDelete(null)} disabled={folderBusy} autoFocus>Cancelar</button>
              <button type="button" className="btn danger" onClick={() => removeFolder(folderDelete.node)} disabled={folderBusy}>Excluir pasta</button>
            </div>
          </div>
        </div>
      )}
      {update && (
        <UpdateDialog update={update} onLater={() => { skip(update.version); setUpdate(null); }} />
      )}
      {!compose && !dialog && !settingsOpen && !update && (
        <button className="fab mobile-only" onClick={writeNew} aria-label="Escrever">
          <NotePencil size={22} weight="bold" />
        </button>
      )}
      <Toaster />
      <div className="version" aria-hidden>v{build.version} · {build.builtAt}</div>
    </div>
  );
}
