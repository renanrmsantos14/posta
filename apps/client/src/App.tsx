import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowBendUpLeft, ArrowBendDoubleUpLeft, ArrowBendUpRight, ArrowsClockwise, Archive, Copy, FolderOpen, FolderSimple, NotePencil, Star, Tag, Trash, EnvelopeSimple, EnvelopeSimpleOpen, FloppyDisk, Printer, PushPin, Clock, ShieldWarning, Prohibit, FunnelSimple } from "@phosphor-icons/react";
import { api, isTauri, type Account, type Attachment, type MailDelta, type MessageDetail, type MessageSummary, type OutAttachment, type OutboxItem, type Role, type Settings, type StoredDraft, type UnreadCounts, type UserFolder, type View } from "./api";
import { AccountDialog } from "./AccountDialog";
import { colorKey } from "./accountColors";
import { accountColor, accountLabel, labelKey } from "./accountMeta";
import { NavDrawer, useDrawer } from "./NavDrawer";
import { parseCategories } from "./Categories";
import { FolderDialog, type FolderDialogState } from "./FolderDialog";
import { SearchDialog, type SearchDialogState } from "./SearchDialog";
import { newSearchId, parseSavedSearches, serializeSavedSearches, suggestName, type SavedSearch } from "./savedSearches";
import { OutboxPane, useQueue } from "./Offline";
import { DraftList, ScheduledList } from "./DraftLists";
import { boxView, catView, parseBox, parseCat, roleOf } from "./views";
import { Composer, type ComposeInit } from "./Composer";
import { aliasFor, aliasLines, addrOf } from "./aliases";
import { justSwiped, swipeProps, type SwipeAction, type SwipeDir } from "./swipe";
import { escapeHtml, textToHtml } from "./richtext";
import { Toaster, toast } from "./toast";
import { watchOpenTarget, type OpenTarget } from "./notificationOpen";
import { applyAppearance, applyStyle, parseCustomThemes, themePref, watchSystem } from "./themes";
import { FOLDERS, LOCAL_VIEWS, SHOWS_UNREAD, folderKey, folderTree, parseOpenMap, viewInfo, withStarred, type Dest, type FolderNode } from "./folders";
import { DragGhost, placeGhost } from "./DragGhost";
import { canDragOut, dragOut } from "./dragout";
import { dropViewAt, trackPointerDrag } from "./pointerdrag";
import { Resizer } from "./Resizer";
import { SettingsPage } from "./SettingsPage";
import { UpdateDialog } from "./UpdateDialog";
import { autoCheck, checkUpdate, CHECK_EVERY_MS, skip, skipped, type Update } from "./updater";
import { addFilter, hasFilters, parseRules, parseTemplates, whenLabel, whenPresets, type ScheduledItem } from "./productivity";
import { AppShell, RouteLayout, PageLayout } from "./layout/primitives";
import { TopBar, type SearchScope } from "./TopBar";
import { CommandBar } from "./CommandBar";
import { buildCommands, hiddenTools } from "./commands";
import { Rail } from "./Rail";
import { MobileActions } from "./MobileActions";
import { Fab, useHideOnScroll } from "./Fab";
import { BulkBar, ListHead } from "./ListParts";
import { usePullToRefresh } from "./PullToRefresh";
import { useNarrow } from "./layout/narrow";
import { ReaderPane, splitAddrs } from "./ReaderPane";
import { MessageList, type Density, type ListHandle, type ListRow } from "./MessageList";
import { PAGE, useMessageList } from "./useMessageList";
import { ContextMenu, type MenuItem, type MenuState } from "./ContextMenu";
import { ContactCard, parseContact, type ContactState } from "./ContactCard";
import { fmtLong } from "./dates";
import { isFocused, repliedThreads } from "./focus";
import { dropUndo, popUndo, pushUndo } from "./undo";
import { isTyping, ownsKeys, shortcutOf, type ShortcutAction } from "./shortcuts";
import build from "./version.json";

/** O WebView do Android não tem impressão por `window.print`. */
const ANDROID = /Android/i.test(navigator.userAgent);
// Rede de segurança: a chegada normal vem do IDLE (evento `mail-changed`).
const SYNC_EVERY_MS = 300_000;
/** Janela inicial das pastas fora da Caixa de entrada, e quanto cresce ao rolar até o fim. */
const KEEP = 1000;
const KEEP_STEP = 500;
const UNDO_MS = 4000;
/** Depois de um `mail-delta`, o `mail-changed` que o acompanha não recarrega a lista por este tempo. */
const DELTA_GRACE_MS = 2000;
/** Quantos emails abertos ficam em memória para reabrir na hora. */
const DETAIL_CACHE = 50;
/** Depois disto sem resposta, o leitor mostra erro com "Tentar de novo" em vez de esperar para sempre. */
const OPEN_TIMEOUT_MS = 25_000;
/** Larguras padrão e limites das colunas (px); o resto da janela fica com a leitura. */
const RAIL = { def: 232, min: 180, max: 360 };
const LIST = { def: 360, min: 280, max: 720 };
const LIST_H = { def: 320, min: 140 };
const MIN_READER = 360;
const DEFAULT_FAVORITES: View[] = ["inbox", "starred"];

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

type Row = ListRow;

/** Separa "A <a@x>, B <b@x>" em endereços (vírgulas dentro de aspas não separam). */

export default function App() {
  const [accounts, setAccounts] = useState<Account[] | null>(null);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<MessageDetail | null>(null);
  /** Falha ao abrir a mensagem selecionada (ex.: sem rede e sem o corpo baixado): o leitor mostra o motivo e "Tentar de novo". */
  const [openError, setOpenError] = useState<{ id: number; msg: string } | null>(null);
  /** Mensagem cujo corpo está sendo buscado agora (o leitor mostra "Abrindo…" em vez de "Selecione uma mensagem"). */
  const [opening, setOpening] = useState<number | null>(null);
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
  /** Seção que a Configuração abre (ex.: "rules" depois de "Criar regra"). */
  const [settingsSection, setSettingsSection] = useState<string | undefined>();
  /** Só a mensagem, ocupando a janela (clique duplo, botão expandir ou painel desligado). */
  const [full, setFull] = useState(false);
  /** As preferências chegam do backend: até lá a janela fica invisível para não piscar o tema. */
  const [prefsReady, setPrefsReady] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  /** Remonta o painel a cada abertura, mesmo com o mesmo rascunho. */
  const [composeKey, setComposeKey] = useState(0);
  /** Cascata só na primeira abertura; buscas e atualizações trocam a lista sem animar. */
  const [intro, setIntro] = useState(true);
  const [folder, setFolder] = useState<View>("inbox");
  const [folderList, setFolderList] = useState<(Role | "starred")[]>(["inbox", "starred"]);
  /** Janela da pasta atual no servidor e total que o servidor informou. */
  const [keep, setKeep] = useState(KEEP);
  const [serverTotal, setServerTotal] = useState<number | null>(null);
  const [loadingFolder, setLoadingFolder] = useState(true);
  /** Não lidas por conta (id) e papel de pasta: fonte única; o total por papel deriva daqui. */
  const [acctCounts, setAcctCounts] = useState<Record<string, UnreadCounts>>({});
  const counts = useMemo(() => {
    const total: UnreadCounts = {};
    for (const own of Object.values(acctCounts)) for (const [role, n] of Object.entries(own)) total[role as Role] = (total[role as Role] ?? 0) + (n ?? 0);
    return total;
  }, [acctCounts]);
  /** Seleção múltipla (checkbox / Shift+clique). */
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const anchor = useRef<number | null>(null);
  /** Estrela que acabou de ser clicada com o mouse: só ela anima. */
  const [popId, setPopId] = useState<number | null>(null);
  /** Mensagens da Lixeira à espera de "Excluir de vez?". */
  const [confirmDelete, setConfirmDelete] = useState<MessageSummary[] | null>(null);
  const [confirmPhish, setConfirmPhish] = useState<MessageSummary[] | null>(null);
  /** Rascunhos locais, a configuração (assinaturas, notificações) e os filtros de visão. */
  const [drafts, setDrafts] = useState<StoredDraft[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const [showDrafts, setShowDrafts] = useState(false);
  const [showScheduled, setShowScheduled] = useState(false);
  const [scheduled, setScheduled] = useState<ScheduledItem[]>([]);
  const [scope, setScope] = useState<SearchScope>("folder");
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [contact, setContact] = useState<ContactState | null>(null);
  /** Fios em que o usuário já escreveu (para a aba Foco). */
  const [replied, setReplied] = useState<Set<string>>(new Set());
  const [serverBusy, setServerBusy] = useState(false);
  const [fromServer, setFromServer] = useState(false);
  /** Caixa de saída: usa o mesmo painel dos rascunhos (`showDrafts` junto) para mostrar outra lista. */
  const [showOutbox, setShowOutbox] = useState(false);
  const [outbox, setOutbox] = useState<OutboxItem[]>([]);
  /** Conta ativa: as pastas da seção de uma conta abrem só ela; null = visões de "Todas as contas". */
  const [accountFilter, setAccountFilter] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  /** Mensagens do fio aberto (da mais antiga à mais nova), as já abertas por expansão e quais estão expandidas. */
  const [thread, setThread] = useState<MessageSummary[]>([]);
  const [bodies, setBodies] = useState<Record<number, MessageDetail>>({});
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  /** Pastas do usuário (todas as contas), o diálogo de criar/renomear e a exclusão à espera de confirmação. */
  const [userFolders, setUserFolders] = useState<UserFolder[]>([]);
  const [folderDialog, setFolderDialog] = useState<FolderDialogState | null>(null);
  const [searchDialog, setSearchDialog] = useState<SearchDialogState | null>(null);
  const [folderDelete, setFolderDelete] = useState<{ node: FolderNode; count: number } | null>(null);
  const [folderBusy, setFolderBusy] = useState(false);
  /** Arrastar da lista para uma pasta: mensagens em arrasto e alvo sob o ponteiro. */
  const dragging = useRef<MessageSummary[]>([]);
  const [dragIds, setDragIds] = useState<Set<number>>(new Set());
  const [dropKey, setDropKey] = useState<string | null>(null);
  /** Fantasma que segue o ponteiro (contagem; "+" quando Ctrl copia) e a posição dele, fora do estado. */
  const [ghost, setGhost] = useState<{ count: number; copy: boolean; label: string } | null>(null);
  const ghostEl = useRef<HTMLDivElement | null>(null);
  const fab = useHideOnScroll();
  const narrow = useNarrow();
  const ghostPos = useRef({ x: 0, y: 0 });
  /** Não lidas por pasta do usuário (chave de `folderKey`). */
  const [folderUnread, setFolderUnread] = useState<Map<string, number>>(new Map());
  const folderRef = useRef(folder);
  /** Verdadeiro enquanto uma ação de deslizar roda (takeOut não troca a seleção). */
  const swiping = useRef(false);
  folderRef.current = folder;
  const loadingMore = useRef(false);
  /** Mensagens no intervalo do "Desfazer": ainda estão no banco local, mas não devem voltar à lista. */
  const pendingMove = useRef<Set<number>>(new Set());
  const appRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<ListHandle>(null);
  /** Emails já abertos (mais recentes por último): reabrir não passa pelo backend. */
  const detailCache = useRef(new Map<number, MessageDetail>());
  const selectedRef = useRef<number | null>(null);

  const refreshCounts = useCallback(() => {
    api.unreadCounts().then(setAcctCounts).catch(() => {});
    api.userFolderUnread().then((l) => setFolderUnread(new Map(l.map((x) => [folderKey(x), x.unread])))).catch(() => {});
  }, []);
  const { messages, setMessages, results, setResults, hasMore, freshIds, reload, loadPage, applyDelta, resetKnown } = useMessageList(folder, pendingMove, refreshCounts);
  /** Tamanho atual da lista, para os "Desfazer" recarregarem sem encolher a janela. */
  const listLen = useRef(0);
  listLen.current = messages.length;

  const grouped = settings.group !== "0";
  const shown = results ?? messages;
  /** Busca em "Todas as caixas": junta as contas mesmo com uma conta ativa. */
  const searchAll = !!results && scope === "all" && (accounts?.length ?? 0) > 1;
  const byAccount = useMemo(() => (accountFilter && !searchAll ? shown.filter((m) => m.account === accountFilter) : shown), [shown, accountFilter, searchAll]);
  /** Foco/Outros só na Caixa de entrada sem busca; a escolha da aba fica salva. */
  const tabsOn = folder === "inbox" && settings.focusTabs !== "0" && !results;
  const tab = settings["inbox:tab"] === "other" ? "other" : "focused";
  const visible = useMemo(() => (tabsOn ? byAccount.filter((m) => isFocused(m, replied) === (tab === "focused")) : byAccount), [byAccount, tabsOn, tab, replied]);
  const tabUnread = useMemo(() => {
    if (!tabsOn) return { focused: 0, other: 0 };
    let focused = 0, other = 0;
    for (const m of byAccount) if (!m.seen) (isFocused(m, replied) ? focused++ : other++);
    return { focused, other };
  }, [byAccount, tabsOn, replied]);
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
  /** Só a pasta real sincroniza com o servidor; "Com estrela", "all" (busca em todas as caixas) e categorias são visões do banco local. */
  const remote = !LOCAL_VIEWS.includes(folder) && !parseCat(folder);
  const unreadOf = (r: Role | "starred") => (r !== "starred" && SHOWS_UNREAD.includes(r) ? counts[r] : 0);
  const multi = (accounts?.length ?? 0) > 1;
  /** Apelido da conta (configuração `label:<id>`) ou o padrão; cor própria ou da paleta. */
  const acctMeta = useMemo(
    () => new Map((accounts ?? []).map((a) => [a.id, { label: accountLabel(accounts, a.id, settings), color: accountColor(accounts, a.id, settings) }])),
    [accounts, settings],
  );
  const acctLabel = (id: string) => acctMeta.get(id)?.label ?? id;
  const acctColor = (id: string) => acctMeta.get(id)?.color ?? accountColor(accounts, id, settings);
  /** Conta de que a visão aberta faz parte: pastas do usuário pertencem a uma; as demais seguem a conta ativa. */
  const scopeAccount = multi ? parseBox(folder)?.account ?? accountFilter : null;
  const categories = useMemo(() => parseCategories(settings.categories), [settings.categories]);
  const savedSearches = useMemo(() => parseSavedSearches(settings.searches), [settings.searches]);
  const templates = useMemo(() => parseTemplates(settings.templates), [settings.templates]);
  const fullTree = useMemo(() => folderTree(userFolders), [userFolders]);
  const tree = useMemo(() => (accountFilter ? fullTree.filter((f) => f.account === accountFilter) : fullTree), [fullTree, accountFilter]);
  const info = viewInfo(folder, userFolders, categories);
  // Celular: sempre remetente, hora e assunto (sem prévia), com altura boa para o dedo.
  const density: Density = !narrow && (settings.density === "compact" || settings.density === "comfortable") ? settings.density : "medium";
  const favorites = useMemo<View[]>(() => {
    let raw: unknown = DEFAULT_FAVORITES;
    try { if (settings.favorites) raw = JSON.parse(settings.favorites); } catch { /* valor inválido: volta ao padrão */ }
    const list = Array.isArray(raw) ? (raw.filter((v) => typeof v === "string") as View[]) : DEFAULT_FAVORITES;
    return list.filter((v) =>
      Object.hasOwn(FOLDERS, v) ? folderList.includes(v as Role | "starred")
        : parseBox(v) ? userFolders.some((f) => boxView(f.account, f.name) === v)
        : parseCat(v) ? categories.some((c) => catView(c.id) === v) : false);
  }, [settings.favorites, folderList, userFolders, categories]);

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
  const canReport = (m: MessageSummary) => m.role !== "junk" && folderList.includes("junk");
  const reportPhishing = (ms: MessageSummary[]) => { const l = ms.filter(canReport); if (l.length) setConfirmPhish(l); };
  const canTrash = (m: MessageSummary) => m.role === "trash" || folderList.includes("trash");

  /** Ajusta o contador na hora; `refreshCounts` confere com o banco depois. */
  const bumpCounts = (ms: MessageSummary[], delta: number) => {
    // Soma os deltas por conta e papel antes de copiar o estado: um único setState por lote.
    const by = new Map<string, number>();
    for (const m of ms) if (m.role) by.set(`${m.account}\u0000${m.role}`, (by.get(`${m.account}\u0000${m.role}`) ?? 0) + delta);
    setAcctCounts((c) => {
      const next = { ...c };
      for (const [k, d] of by) {
        const [account, role] = k.split("\u0000") as [string, Role];
        const own = (next[account] = { ...next[account] });
        own[role] = Math.max(0, (own[role] ?? 0) + d);
      }
      return next;
    });
  };

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
      if (r.offline) {
        refreshNet();
        if (manual) toast("Sem conexão. Você vê o que já está salvo.", "info");
        return;
      }
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

  // Rede e fila offline: ao voltar a conexão, a fila é reaplicada e as listas recarregam.
  const { status: net, refresh: refreshNet } = useQueue(() => { refreshCounts(); reload(messages.length).catch(() => {}); });
  const refreshOutbox = useCallback(() => { api.listOutbox().then(setOutbox).catch(() => {}); refreshNet(); }, [refreshNet]);
  useEffect(() => { if (showOutbox || net.outbox) refreshOutbox(); }, [showOutbox, net.outbox, refreshOutbox]);

  const refreshDrafts = useCallback(() => api.listDrafts().then(setDrafts).catch(() => {}), []);
  const refreshScheduled = useCallback(() => api.listScheduled().then(setScheduled).catch(() => {}), []);

  useEffect(() => {
    api.listAccounts().then(setAccounts).catch((e) => window.postaBootLog?.(`ui: list_accounts falhou: ${e}`));
    api.folders().then((r) => setFolderList(withStarred(r))).catch(() => {});
    api.userFolders().then(setUserFolders).catch(() => {});
    refreshDrafts();
    refreshScheduled();
    api.settings().then((s) => {
      setSettings(s);
      // Android 13+ só mostra notificações depois do aceite; no desktop a chamada é inofensiva.
      if (s.notify !== "0") api.listAccounts().then((a) => (a.length ? api.requestNotifyPermission() : false)).catch(() => {});
    }).catch((e) => window.postaBootLog?.(`ui: get_settings falhou: ${e}`)).finally(() => setPrefsReady(true));
  }, [refreshDrafts, refreshScheduled]);

  // Fechar ou enviar o painel muda os rascunhos salvos e a fila de agendados.
  useEffect(() => { if (!compose) { refreshDrafts(); refreshScheduled(); } }, [compose, refreshDrafts, refreshScheduled]);
  // Arquivo solto fora do compositor não pode navegar a janela para ele (o compositor trata o próprio soltar).
  useEffect(() => {
    const stop = (e: DragEvent) => { if (Array.from(e.dataTransfer?.types ?? []).includes("Files")) e.preventDefault(); };
    window.addEventListener("dragover", stop);
    window.addEventListener("drop", stop);
    return () => { window.removeEventListener("dragover", stop); window.removeEventListener("drop", stop); };
  }, []);

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
    resetKnown();
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
  }, [reload, syncFolder, folder, remote, resetKnown]);

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
    refreshScheduled();
    if (!LOCAL_VIEWS.includes(folderRef.current)) return;
    // O `mail-delta` que vem junto já atualizou a lista por id: só recarrega tudo quando ele não veio (fallback).
    if (Date.now() - lastDelta.current < DELTA_GRACE_MS) return void (fetched && toast(plural(fetched, "mensagem nova", "mensagens novas"), "success"));
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

  // `mail-delta`: aplica inserções, mudanças e remoções por id (no navegador, o mock dispara `mock-delta`).
  const lastDelta = useRef(0);
  const onDelta = useRef((_d: MailDelta) => {});
  onDelta.current = (d: MailDelta) => {
    if (!roleOf(folderRef.current)) return;
    lastDelta.current = Date.now();
    applyDelta(d).catch(() => { lastDelta.current = 0; reload(listLen.current).catch(() => {}); });
  };
  useEffect(() => {
    if (!isTauri) {
      const on = (e: Event) => onDelta.current((e as CustomEvent<MailDelta>).detail);
      window.addEventListener("mock-delta", on);
      return () => window.removeEventListener("mock-delta", on);
    }
    let off: (() => void) | undefined;
    let alive = true;
    import("@tauri-apps/api/event")
      .then(({ listen }) => listen<MailDelta>("mail-delta", (e) => onDelta.current(e.payload)))
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

  // Clique numa notificação de email novo: vai à conta e à pasta e abre o email; se ele saiu da pasta, fica a pasta e um aviso.
  const openTarget = useRef<(t: OpenTarget) => Promise<void>>(async () => {});
  openTarget.current = async (t) => {
    setQuery("");
    setResults(null);
    navigate(t.folder, t.account);
    if (t.id == null) return;
    const d = await api.open(t.id).catch(() => null);
    if (d && d.account === t.account && d.role === roleOf(t.folder)) return select(t.id);
    toast("Esse email não está mais aqui. Abri a pasta.", "info");
  };
  const accountsReady = accounts !== null;
  useEffect(() => (accountsReady ? watchOpenTarget((t) => void openTarget.current(t)) : undefined), [accountsReady]);

  // Busca local a cada tecla: sem debounce, o SQLite responde em < 1 ms.
  useEffect(() => {
    setFromServer(false);
    if (!query.trim()) return setResults(null);
    let alive = true;
    const where: View = scope === "all" && (accounts?.length ?? 0) > 1 ? "all" : folder;
    (hasFilters(query) ? api.searchFiltered(query, where) : api.search(query, where)).then((r) => alive && setResults(r));
    return () => { alive = false; };
  }, [query, folder, scope, accounts?.length]);

  // Foco/Outros: fios em que já escrevi saem dos enviados.
  useEffect(() => {
    if (folder !== "inbox" || settings.focusTabs === "0") return;
    let alive = true;
    api.list("sent", 500, 0).then((s) => alive && setReplied(repliedThreads(s))).catch(() => {});
    return () => { alive = false; };
  }, [folder, settings.focusTabs, syncing]);

  // Mock (navegador): `__mockMany`/`__mockOffline` avisam que o banco simulado mudou.
  useEffect(() => {
    if (isTauri) return;
    const on = () => { reload(listLen.current).catch(() => {}); };
    window.addEventListener("mock-changed", on);
    return () => window.removeEventListener("mock-changed", on);
  }, [reload]);

  // Fio da mensagem aberta: recarrega quando chega mensagem nova nele.
  const threadCount = current?.thread_count;
  useEffect(() => {
    if (selectedId === null || !grouped) return setThread([]);
    let alive = true;
    api.thread(selectedId).then((t) => alive && setThread(t)).catch(() => {});
    return () => { alive = false; };
  }, [selectedId, grouped, threadCount]);
  useEffect(() => { setBodies({}); setExpanded(new Set()); }, [selectedId]);

  // Com a conta removida só resta uma caixa: não há mais conta ativa.
  useEffect(() => { if (accounts && accounts.length < 2 && accountFilter) setAccountFilter(null); }, [accounts, accountFilter]);

  function openFolder(role: View) {
    // "all" (visão antiga de todas as caixas) agora é a Entrada sem conta ativa.
    if (role === "all") role = "inbox";
    setShowDrafts(false);
    setShowScheduled(false);
    setShowOutbox(false);
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

  /** Abre a visão na conta ativa `account` (null = todas as contas). Pastas do usuário trazem a própria conta. */
  function navigate(view: View, account: string | null = null) {
    const acc = multi ? parseBox(view)?.account ?? account : null;
    setAccountFilter(acc);
    if (acc && !openAccounts.includes(acc)) setOpenAccounts(oneAtATime ? [acc] : [...openAccounts, acc]);
    openFolder(view);
  }
  /** "Uma por vez" (sanfona): abrir uma conta fecha as outras. */
  const oneAtATime = settings.railAccounts === "one";
  /** Seções de conta abertas na barra lateral (várias ao mesmo tempo, guardadas como as outras preferências). */
  const openAccounts = (settings["rail:accounts"] ?? settings["rail:account"] ?? "").split("\n").filter((id) => accounts?.some((a) => a.id === id)).slice(0, oneAtATime ? 1 : undefined);
  const setOpenAccounts = (ids: string[]) => changeSetting("rail:accounts", ids.join("\n"));
  const drawer = useDrawer();
  const menuRef = useRef<HTMLButtonElement>(null);

  const remember = (d: MessageDetail) => {
    const c = detailCache.current;
    c.delete(d.id);
    c.set(d.id, d);
    if (c.size > DETAIL_CACHE) c.delete(c.keys().next().value!);
  };
  const detailOf = async (id: number) => {
    const hit = detailCache.current.get(id);
    if (hit) return hit;
    const d = await api.open(id);
    if (d) remember(d);
    return d;
  };

  /** Marca como lida só na tela (o servidor já sabe: `api.open` marca). */
  const markLocalSeen = (id: number) => {
    const upd = (l: MessageSummary[]) => l.map((m) => (m.id === id && !m.seen ? { ...m, seen: true } : m));
    setMessages(upd);
    setResults((r) => r && upd(r));
    refreshCounts();
  };

  /** `api.open` com prazo: uma conexão presa não deixa o leitor esperando para sempre (o resultado atrasado é descartado). */
  const openWithin = (id: number, ms: number) => {
    let timer: ReturnType<typeof setTimeout>;
    const late = new Promise<never>((_, reject) => { timer = setTimeout(() => reject("Demorou demais para abrir este email."), ms); });
    return Promise.race([api.open(id), late]).finally(() => clearTimeout(timer));
  };

  async function select(id: number, keyboard = false) {
    setViaKeyboard(keyboard);
    setSelectedId(id);
    selectedRef.current = id;
    setReaderOpen(true);
    setOpenError(null);
    const hit = detailCache.current.get(id);
    let d: MessageDetail | null;
    if (hit) {
      d = hit;
      setDetail(hit);
      remember(hit);
      const sum = rowOf.get(id)?.members.find((m) => m.id === id);
      if (sum && !sum.seen) void markSeen([sum], true, false);
    } else {
      setOpening(id);
      try {
        d = await openWithin(id, OPEN_TIMEOUT_MS);
      } catch (e) {
        if (selectedRef.current === id) { setDetail(null); setOpenError({ id, msg: String(e) }); }
        return;
      } finally {
        setOpening((cur) => (cur === id ? null : cur));
      }
      if (selectedRef.current !== id) return;
      setDetail(d);
      if (d) { remember(d); markLocalSeen(id); }
    }
    if (d) {
      // Abrir a conversa lê o fio todo.
      const rest = (rowOf.get(id)?.members ?? []).filter((m) => m.id !== id && !m.seen);
      if (rest.length) void markSeen(rest, true, false);
    }
  }

  /** Abre uma mensagem anterior do fio (marca como lida) ou a recolhe. */
  async function toggleThreadMsg(id: number) {
    if (expanded.has(id)) return setExpanded((s) => { const n = new Set(s); n.delete(id); return n; });
    if (!bodies[id]) {
      try {
        const d = await detailOf(id);
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

  function writeNew(to = "") {
    if (!accounts?.length) return setDialog(true);
    openComposer({ title: "Nova mensagem", account: accounts[0].id, reply_to: null, to, subject: "", body: "", html: `<div><br></div>${signatureFor(accounts[0].id)}` });
  }

  function openDraft(d: StoredDraft) {
    if (!accounts?.length) return;
    const account = accounts.some((a) => a.id === d.account) ? d.account : accounts[0].id;
    openComposer({ title: d.reply_to !== null ? "Responder" : "Nova mensagem", draftId: d.id, account, reply_to: d.reply_to, to: d.to, cc: d.cc, bcc: d.bcc, subject: d.subject, body: d.body, html: d.html || undefined });
  }

  function openScheduled() {
    setShowScheduled(true);
    setShowOutbox(false);
    setShowDrafts(false);
    setReaderOpen(false);
    refreshScheduled();
  }

  function openOutbox() {
    setShowDrafts(true);
    setShowOutbox(true);
    setShowScheduled(false);
    setReaderOpen(false);
    refreshOutbox();
  }

  function openDrafts() {
    setShowDrafts(true);
    setShowScheduled(false);
    setShowOutbox(false);
    setReaderOpen(false);
    refreshDrafts();
  }

  /** Tema, destaque e fonte vivem no <html>. Antes de as configurações chegarem vale o que o public/theme-init.js aplicou. */
  const [accentAdjusted, setAccentAdjusted] = useState(false);
  const customThemes = useMemo(() => parseCustomThemes(settings.customThemes), [settings.customThemes]);
  useEffect(() => {
    if (!prefsReady) return;
    const pref = themePref(settings.theme, customThemes);
    const apply = () => setAccentAdjusted(applyAppearance(pref, settings.accent, customThemes)?.adjusted ?? false);
    apply();
    return pref === "system" ? watchSystem(apply) : undefined;
  }, [prefsReady, settings.theme, settings.accent, customThemes]);
  useEffect(() => {
    if (prefsReady) applyStyle(settings);
  }, [prefsReady, settings.radius, settings.shadow, settings.font, settings.railDensity]);
  useEffect(() => {
    const root = document.documentElement;
    if (settings.size === "s" || settings.size === "l") root.dataset.size = settings.size;
    else delete root.dataset.size;
  }, [settings.size]);

  // Tela cheia só existe com uma mensagem aberta e fora das Configurações.
  useEffect(() => { if (full && !current) setFull(false); }, [full, current]);

  /** Muda só na tela (arrasto das colunas); `changeSetting` grava ao soltar. */
  const previewSetting = (key: string, value: string) => setSettings((s) => ({ ...s, [key]: value }));

  const openRaw = settings["folders:open"];
  const openMap = useMemo(() => parseOpenMap(openRaw), [openRaw]);
  const openRef = useRef(openMap);
  openRef.current = openMap;
  /** Abre ou fecha uma pasta da árvore e guarda a escolha (por conta + pasta). */
  const toggleFolder = useCallback((n: FolderNode, open: boolean) => {
    openRef.current = { ...openRef.current, [folderKey(n)]: open };
    const next = JSON.stringify(openRef.current);
    setSettings((s) => ({ ...s, "folders:open": next }));
    api.setSetting("folders:open", next).catch((e) => toast(`Não consegui salvar a configuração: ${e}`, "error"));
  }, []);

  function changeSetting(key: string, value: string) {
    setSettings((s) => ({ ...s, [key]: value }));
    api.setSetting(key, value).catch((e) => toast(`Não consegui salvar a configuração: ${e}`, "error"));
  }

  const savedSearchesRef = useRef(savedSearches);
  savedSearchesRef.current = savedSearches;
  const writeSearches = (list: SavedSearch[]) => changeSetting("searches", serializeSavedSearches(list));
  const activeSearch = savedSearches.find((s) => s.query === query.trim() && s.scope === scope && (scope === "all" || s.folder === folder))?.id ?? null;
  function openSearch(s: SavedSearch) {
    navigate(s.folder as View, s.account);
    setScope(s.scope === "all" && multi ? "all" : "folder");
    setQuery(s.query);
  }
  function submitSearch(name: string) {
    if (!searchDialog) return;
    if (searchDialog.mode === "rename") {
      writeSearches(savedSearches.map((s) => (s.id === searchDialog.id ? { ...s, name } : s)));
    } else {
      writeSearches([...savedSearches, { id: newSearchId(), name, query: searchDialog.query.trim(), folder, scope, account: accountFilter }]);
      toast(`Pesquisa “${name}” salva`, "info");
    }
  }
  function deleteSearch(s: SavedSearch) {
    const index = savedSearches.findIndex((x) => x.id === s.id);
    writeSearches(savedSearches.filter((x) => x.id !== s.id));
    // Desfazer reinsere só esta pesquisa, preservando mudanças feitas depois da exclusão.
    toast(`Pesquisa “${s.name}” excluída`, "info", { action: { label: "Desfazer", onClick: () => {
      const now = savedSearchesRef.current;
      if (now.some((x) => x.id === s.id)) return;
      const next = [...now];
      next.splice(Math.min(Math.max(index, 0), next.length), 0, s);
      writeSearches(next);
    } } });
  }
  const searchContext = (x: number, y: number, s: SavedSearch) => setMenu({ x, y, label: "Ações da pesquisa", items: [
    { label: "Renomear", icon: NotePencil, onSelect: () => setSearchDialog({ mode: "rename", id: s.id, query: s.query, name: s.name }) },
    { label: "Excluir pesquisa", icon: Trash, danger: true, onSelect: () => deleteSearch(s) },
  ] });

  const imagesFrom = useMemo(() => new Set(aliasLines(settings["images:trusted"]).map(addrOf)), [settings["images:trusted"]]);
  function trustSender(addr: string) {
    const before = settings["images:trusted"] ?? "";
    changeSetting("images:trusted", [...aliasLines(before), addr].join("\n"));
    toast(`Imagens de ${addr} serão sempre mostradas`, "info", { action: { label: "Desfazer", onClick: () => changeSetting("images:trusted", before) } });
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
    setGhost(null);
  }

  /** Alvo de soltar mensagens arrastadas da lista; a pasta aberta não é alvo (o hit-test lê `data-drop-view`). */
  function dropProps(key: string, view: string): Record<string, unknown> {
    if (view === folder || showDrafts || showScheduled) return {};
    return { "data-drop-view": view, "data-drop": dropKey === key || undefined };
  }

  /** Destino de soltar sobre `view`, ou null quando nenhuma das mensagens arrastadas poderia ir para lá. */
  function destOf(view: string, ms: MessageSummary[]): Dest | null {
    // Pasta da seção de uma conta (`papel|conta`): só recebe mensagens daquela conta.
    const sep = view.indexOf("|");
    if (sep >= 0) return ms.every((m) => m.account === view.slice(sep + 1)) ? destOf(view.slice(0, sep), ms) : null;
    const box = parseBox(view as View);
    if (box) return ms.some((m) => m.account === box.account) ? { account: box.account, name: box.name, label: viewInfo(view as View, userFolders, categories).label } : null;
    return Object.hasOwn(FOLDERS, view) && view !== "starred" && view !== "all" && ms.some((m) => m.role !== view) ? (view as Role) : null;
  }

  /** Copia (Ctrl ao soltar): a original fica onde está e não há "Desfazer". */
  async function copyOut(ms: MessageSummary[], to: Dest) {
    const mine = typeof to === "string" ? ms.filter((m) => m.role !== to) : ms.filter((m) => m.account === to.account);
    if (typeof to !== "string" && mine.length < ms.length) toast("Só dá para copiar para pastas da mesma conta", "info");
    if (!mine.length) return;
    const ids = mine.map((m) => m.id);
    const name = typeof to === "string" ? FOLDERS[to].label : to.label;
    try {
      if (typeof to === "string") await api.copyMany(ids, to);
      else await api.copyToFolder(ids, to.account, to.name);
      toast(`${plural(ids.length, "mensagem copiada", "mensagens copiadas")} para ${name}`, "info");
      refreshCounts();
    } catch (e) {
      toast(`Não foi possível copiar: ${e}`, "error");
    }
  }

  /** Anexo do leitor arrastado para fora da janela: o sistema recebe o arquivo (só no app instalado). */
  function dragAttachment(e: React.PointerEvent, id: number, att: Attachment) {
    if (!canDragOut) return;
    trackPointerDrag(e, {
      leave: async (released) => {
        try {
          await dragOut(() => api.prepareDragAttachment(id, att.index), released);
        } catch (err) {
          toast(`Não consegui arrastar o anexo: ${err}`, "error");
        }
      },
    });
  }

  /** Arrasto da lista com o mouse: fantasma, alvo destacado, solta = mover (Ctrl = copiar); fora da janela vira arrasto nativo de .eml. */
  function startRowDrag(e: React.PointerEvent, row: ListRow) {
    const ms = checked.has(row.head.id) ? checkedList : row.members;
    const hit = (x: number, y: number) => {
      const v = dropViewAt(x, y);
      return v && destOf(v, ms) ? v : null;
    };
    trackPointerDrag(e, {
      start: (x, y) => {
        dragging.current = ms;
        ghostPos.current = { x, y };
        setDragIds(new Set(ms.map((m) => m.id)));
        setGhost({ count: ms.length, copy: e.ctrlKey, label: row.head.subject });
      },
      move: (x, y, copy) => {
        ghostPos.current = { x, y };
        placeGhost(ghostEl.current, x, y);
        setGhost((g) => (g && g.copy !== copy ? { ...g, copy } : g));
        setDropKey(hit(x, y));
      },
      end: (x, y, copy) => {
        const v = hit(x, y);
        endDrag();
        const dest = v && destOf(v, ms);
        if (!dest) return;
        if (copy) void copyOut(ms, dest);
        else moveToDest(ms, dest);
      },
      cancel: endDrag,
      leave: canDragOut ? async (released) => {
        endDrag();
        try {
          await dragOut(() => api.prepareDragEml(ms.map((m) => m.id)), released);
        } catch (err) {
          toast(`Não consegui arrastar para fora: ${err}`, "error");
        }
      } : undefined,
    });
  }

  async function removeAccount(id: string) {
    await api.removeAccount(id);
    setAccounts((l) => (l ?? []).filter((a) => a.id !== id));
    setSettings((s) => Object.fromEntries(Object.entries(s).filter(([k]) => k !== `signature:${id}` && k !== colorKey(id) && k !== labelKey(id))));
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

  function reply(d: MessageDetail | null = detail) {
    if (!d || !accounts?.length) return;
    const subject = /^re:/i.test(d.subject) ? d.subject : `Re: ${d.subject}`;
    const account = accounts.some((a) => a.id === d.account) ? d.account : accounts[0].id;
    openComposer({ title: "Responder", account, reply_to: d.id, to: d.reply_addr, subject, body: "", html: signatureFor(account) + replyHtml(d), from: replyFrom(d, account) });
  }

  /** Remetente em Para; os demais destinatários da original em Cc, sem as minhas contas e sem repetir. */
  function replyAll(d: MessageDetail | null = detail) {
    if (!d || !accounts?.length) return;
    const own = new Set([...accounts.map((a) => a.username.toLowerCase()), ...accounts.flatMap((a) => aliasLines(settings[`aliases:${a.id}`]).map(addrOf))]);
    const seen = new Set([d.reply_addr.toLowerCase()]);
    const others = [...splitAddrs(d.to_addrs), ...splitAddrs(d.cc_addrs)].filter((a) => {
      const k = a.toLowerCase();
      if (own.has(k) || seen.has(k)) return false;
      seen.add(k);
      return true;
    });
    const subject = /^re:/i.test(d.subject) ? d.subject : `Re: ${d.subject}`;
    const account = accounts.some((a) => a.id === d.account) ? d.account : accounts[0].id;
    openComposer({
      title: "Responder a todos", account, reply_to: d.id,
      to: d.reply_addr, cc: others.join(", "), subject, body: "", html: replyHtml(d), from: replyFrom(d, account),
    });
  }

  async function forward(d: MessageDetail | null = detail) {
    if (!d || !accounts?.length) return;
    const subject = /^(fwd?|enc):/i.test(d.subject) ? d.subject : `Fwd: ${d.subject}`;
    const account = accounts.some((a) => a.id === d.account) ? d.account : accounts[0].id;
    let attachments: OutAttachment[] = [];
    if (d.attachments.length) {
      try {
        attachments = await api.forwardAttachments(d.id);
      } catch (e) {
        toast(`Anexos não incluídos no encaminhamento: ${e}`, "error");
      }
    }
    openComposer({ title: "Encaminhar", account, reply_to: null, to: "", subject, body: "", html: forwardHtml(d), attachments });
  }

  /** Responder/encaminhar a partir de qualquer lugar (atalho, faixa de comandos, menu): usa o email já aberto ou busca. */
  async function act(kind: "reply" | "replyAll" | "forward", id: number | null = selectedId) {
    if (id === null) return;
    const d = detail?.id === id ? detail : await detailOf(id);
    if (!d) return;
    if (kind === "reply") reply(d);
    else if (kind === "replyAll") replyAll(d);
    else void forward(d);
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
          if (r.queued) {
            toast("Sem conexão. A mensagem está na Caixa de saída e sai quando a rede voltar.", "info");
            refreshOutbox();
          } else {
            toast("Mensagem enviada", "success");
          }
          if (r.warning) toast(r.warning, "info");
        } catch (e) {
          toast(`Não foi enviada: ${e}`, "error");
          openComposer({ ...d, restored: true });
        }
      },
    });
  }

  /** Agenda o envio: o app guarda a mensagem e manda na hora marcada (com o Posta aberto). */
  async function schedule(d: ComposeInit, at: number) {
    const { title: _t, restored: _r, draftId, ...draft } = d;
    await api.scheduleSend(draftId ?? crypto.randomUUID(), at, draft);
    if (draftId) api.deleteDraft(draftId).catch(() => {});
    setCompose(null);
    refreshScheduled();
    toast(`Agendada para ${whenLabel(at)}`, "success");
  }

  /** Tira da fila e reabre no Composer; se fechar sem reagendar, o rascunho fica salvo. */
  async function editScheduled(s: ScheduledItem) {
    try {
      const t = await api.takeScheduled(s.id);
      refreshScheduled();
      if (!t) return;
      openComposer({ title: "Editar mensagem agendada", draftId: t.id, account: t.account, reply_to: t.reply_to, to: t.to, cc: t.cc, bcc: t.bcc, subject: t.subject, body: t.body, html: t.html || undefined, attachments: t.attachments });
    } catch (e) {
      toast(`Não foi possível abrir: ${e}`, "error");
    }
  }

  /** Cancelar não perde o texto: a mensagem volta para Rascunhos. */
  async function cancelScheduled(s: ScheduledItem) {
    try {
      const t = await api.takeScheduled(s.id);
      if (t) {
        await api.saveDraft({ id: t.id, account: t.account, reply_to: t.reply_to, to: t.to, cc: t.cc, bcc: t.bcc, subject: t.subject, body: t.body, html: t.html });
        refreshDrafts();
      }
      refreshScheduled();
      toast(t?.attachments?.length ? "Envio cancelado. O texto está em Rascunhos; reanexe os arquivos." : "Envio cancelado. A mensagem está em Rascunhos.", "info");
    } catch (e) {
      toast(`Não foi possível cancelar: ${e}`, "error");
    }
  }

  /** Esconde até `until`; volta ao topo como não lida. Dá para desfazer logo depois. */
  function snooze(ms: MessageSummary[], until: number) {
    const ids = ms.map((m) => m.id);
    const undo = takeOutMany(ms);
    api.snooze(ids, until).then(refreshCounts).catch((e) => { undo(); toast(`Não foi possível adiar: ${e}`, "error"); });
    toast(`Adiada até ${whenLabel(until)}`, "info", {
      action: { label: "Desfazer", onClick: async () => { await api.unsnooze(ids).catch(() => {}); undo(); refreshCounts(); } },
      duration: UNDO_MS,
    });
  }

  const applyPin = (ids: number[], v: boolean) => {
    const set = new Set(ids);
    const f = (l: MessageSummary[]) => l.map((m) => (set.has(m.id) ? { ...m, pinned: v } : m));
    setMessages(f);
    setResults((r) => r && f(r));
  };

  /** Fixar: se todas já estão fixadas, desafixa; senão fixa todas. */
  async function togglePin(ms: MessageSummary[]) {
    if (!ms.length) return;
    const on = !ms.every((m) => m.pinned);
    const ids = ms.map((m) => m.id);
    applyPin(ids, on);
    try {
      await api.setPinned(ids, on);
      await reload(listLen.current);
      const back = async () => { applyPin(ids, !on); await api.setPinned(ids, !on).catch(() => {}); await reload(listLen.current).catch(() => {}); };
      pushUndo(on ? "Fixar" : "Desafixar", back);
      toast(on ? "Fixada no topo" : "Desafixada", "info", { action: { label: "Desfazer", onClick: () => { dropUndo(back); void back(); } }, duration: UNDO_MS });
    } catch (e) {
      applyPin(ids, !on);
      toast(`Não foi possível fixar: ${e}`, "error");
    }
  }

  /** "Buscar no servidor": IMAP SEARCH na pasta atual; o que ainda não estava no aparelho é baixado. */
  async function searchServer() {
    setServerBusy(true);
    try {
      setResults(await api.searchServer(query, folder));
      setFromServer(true);
    } catch (e) {
      toast(`Busca no servidor falhou: ${e}`, "error");
    } finally {
      setServerBusy(false);
    }
  }

  /** Bloquear: regra "remetente → Lixeira", já aplicada à Entrada; "Desfazer" tira só esta regra. */
  function blockSender(m: MessageSummary) {
    const id = `r${Date.now().toString(36)}`;
    const rules = parseRules(settings.rules);
    changeSetting("rules", JSON.stringify([...rules, { id, field: "from", contains: m.from_addr, action: "trash", arg: "", enabled: true }]));
    void api.applyRulesNow().then(() => reload(messages.length)).catch(() => {});
    toast(`${m.from_addr} bloqueado: as mensagens vão para a Lixeira`, "info", { action: { label: "Desfazer", onClick: () => {
      changeSetting("rules", JSON.stringify(parseRules(settingsRef.current.rules).filter((r) => r.id !== id)));
    } } });
  }

  /** Cria a regra desligada, com o remetente preenchido, e abre as Regras para escolher a ação. */
  function ruleFromSender(m: MessageSummary) {
    const rules = parseRules(settings.rules);
    changeSetting("rules", JSON.stringify([...rules, { id: `r${Date.now().toString(36)}`, field: "from", contains: m.from_addr, action: "read", arg: "", enabled: false }]));
    setSettingsSection("rules");
    setSettingsOpen(true);
  }

  async function applyRules() {
    try {
      const n = await api.applyRulesNow();
      toast(n ? `${plural(n, "mensagem atualizada", "mensagens atualizadas")} pelas regras` : "Nenhuma mensagem da Entrada casou com as regras", n ? "success" : "info");
      await reload(messages.length);
      api.userFolders().then(setUserFolders).catch(() => {});
    } catch (e) {
      toast(`Não foi possível aplicar as regras: ${e}`, "error");
    }
  }

  async function loadMore() {
    if (results || loadingMore.current) return;
    loadingMore.current = true;
    try {
      if (hasMore) {
        await loadPage(messages.length);
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
    } else if (next) {
      select(next.id, true);
      const a = document.activeElement;
      if (!a || a === document.body || a.closest(".list")) listRef.current?.reveal(next.id, true);
    } else { setSelectedId(null); setDetail(null); setReaderOpen(false); }
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

  /** Sai da lista na hora; o servidor só recebe quando o "Desfazer" expira. Ctrl+Z vale antes e depois disso. */
  function moveOut(ms: MessageSummary[], to: Dest) {
    const ids = ms.map((m) => m.id);
    const origin = new Map<Role, number[]>();
    ms.forEach((m) => { if (m.role) origin.set(m.role, [...(origin.get(m.role) ?? []), m.id]); });
    const undo = takeOutMany(ms);
    ids.forEach((id) => pendingMove.current.add(id));
    let undone = false;
    const bringBack = () => { undone = true; ids.forEach((id) => pendingMove.current.delete(id)); undo(); };
    const text = typeof to !== "string" ? `Movida para ${to.label}`
      : to === "trash" ? "Movida para a Lixeira" : to === "inbox" ? "Movida para a Caixa de entrada" : to === "archive" ? "Arquivada" : `Movida para ${FOLDERS[to].label}`;
    const label = ms.length === 1 ? text : `${plural(ms.length, "mensagem movida", "mensagens movidas")}`;
    pushUndo(label, bringBack);
    toast(label, "info", {
      action: { label: "Desfazer", onClick: () => { dropUndo(bringBack); bringBack(); } },
      duration: UNDO_MS,
      onClose: async (reason) => {
        if (reason === "action" || undone) return;
        try {
          if (typeof to === "string") await api.moveMany(ids, to);
          else await api.moveToFolder(ids, to.account, to.name);
          ids.forEach((id) => pendingMove.current.delete(id));
          refreshCounts();
          // Depois do envio ao servidor, Ctrl+Z move de volta para a pasta de origem.
          dropUndo(bringBack);
          pushUndo(label, async () => {
            for (const [role, list] of origin) await api.moveMany(list, role);
            await reload(listLen.current);
            refreshCounts();
          });
        } catch (e) {
          dropUndo(bringBack);
          bringBack();
          toast(`Não foi possível mover: ${e}`, "error");
        }
      },
    });
  }

  /** Move para qualquer destino (menu e arrasto). Entre contas diferentes não dá. */
  function moveToDest(ms: MessageSummary[], dest: Dest) {
    if (typeof dest !== "string") {
      const other = ms.filter((m) => m.account !== dest.account);
      if (other.length) toast("Só dá para mover para pastas da mesma conta", "info");
      const same = ms.filter((m) => m.account === dest.account);
      if (same.length) moveOut(same, dest);
      return;
    }
    moveEach(ms.filter((m) => m.role !== dest), () => dest);
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

  const applySeen = (todo: MessageSummary[], seen: boolean) => {
    const ids = new Set(todo.map((m) => m.id));
    const set = (l: MessageSummary[]) => l.map((m) => (ids.has(m.id) ? { ...m, seen } : m));
    setMessages(set);
    setResults((r) => r && set(r));
    setThread(set);
    bumpCounts(todo, seen ? -1 : 1);
  };

  /** Lida/não lida: muda na lista na hora e desfaz se o servidor recusar. `record`: entra no Ctrl+Z e mostra "Desfazer". */
  async function markSeen(ms: MessageSummary[], seen: boolean, record = true) {
    const todo = ms.filter((m) => m.seen !== seen);
    if (!todo.length) return;
    const ids = todo.map((m) => m.id);
    applySeen(todo, seen);
    try {
      await api.setSeen(ids, seen);
      if (record) {
        const back = async () => { applySeen(todo.map((m) => ({ ...m, seen })), !seen); await api.setSeen(ids, !seen).catch(() => {}); refreshCounts(); };
        pushUndo(seen ? "Marcada como lida" : "Marcada como não lida", back);
        toast(ids.length === 1 ? (seen ? "Marcada como lida" : "Marcada como não lida") : `${plural(ids.length, "mensagem marcada", "mensagens marcadas")} como ${seen ? "lidas" : "não lidas"}`, "info",
          { action: { label: "Desfazer", onClick: () => { dropUndo(back); void back(); } }, duration: 3000 });
      }
    } catch (e) {
      applySeen(todo.map((m) => ({ ...m, seen })), !seen);
      toast(`Não foi possível marcar: ${e}`, "error");
    }
    refreshCounts();
  }

  const applyFlag = (ids: number[], on: boolean) => {
    const set = new Set(ids);
    const f = (l: MessageSummary[]) => l.map((m) => (set.has(m.id) ? { ...m, flagged: on } : m));
    setMessages(f);
    setResults((r) => r && f(r));
  };

  /** Estrela/sinalizador: se todas já têm, tira; senão põe em todas. Só o clique do mouse anima. */
  async function toggleStar(ms: MessageSummary[], keyboard = false) {
    if (!ms.length) return;
    const on = !ms.every((m) => m.flagged);
    const ids = ms.filter((m) => m.flagged !== on).map((m) => m.id);
    applyFlag(ids, on);
    if (!keyboard && ms.length === 1) {
      setPopId(ms[0].id);
      setTimeout(() => setPopId(null), 400);
    }
    try {
      await api.setFlagged(ids, on);
      pushUndo(on ? "Sinalizar" : "Remover sinalização", async () => { applyFlag(ids, !on); await api.setFlagged(ids, !on).catch(() => {}); });
    } catch (e) {
      applyFlag(ids, !on);
      toast(`Não foi possível sinalizar: ${e}`, "error");
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
  const notified = useRef(""); // a checagem roda a cada 2 min: não repete o aviso da mesma versão (nem no StrictMode)
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

  // Atualização: procura ao abrir e a cada 2 min, em silêncio; só avisa se houver versão nova não adiada.
  useEffect(() => {
    const run = () => {
      // Em segundo plano não procura (poupa a cota da API do GitHub); ao voltar, procura na hora.
      if (!autoCheck() || document.visibilityState === "hidden") return;
      checkUpdate()
        .then((u) => {
          if (!u || u.version === skipped() || u.version === notified.current) return;
          notified.current = u.version;
          showUpdate(u);
        })
        .catch((e) => console.warn("procurar atualização falhou:", e));
    };
    run();
    const timer = setInterval(run, CHECK_EVERY_MS);
    const onVisible = () => document.visibilityState === "visible" && run();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [showUpdate]);

  // ---- Favoritos, menus de contexto e contato ----

  const toggleFavorite = (v: View) => changeSetting("favorites", JSON.stringify(favorites.includes(v) ? favorites.filter((f) => f !== v) : [...favorites, v]));

  /** Mover para ▸: pastas do sistema que a conta tem e as pastas do usuário. */
  const moveItems = (ms: MessageSummary[]): MenuItem[] => {
    const roles = (["inbox", "archive", "junk", "trash"] as Role[]).filter((r) => folderList.includes(r));
    const items: MenuItem[] = roles.map((r) => ({ label: FOLDERS[r].label, icon: FOLDERS[r].icon, onSelect: () => moveToDest(ms, r) }));
    if (tree.length) {
      items.push({ separator: true });
      for (const n of tree) items.push({ label: `${"· ".repeat(n.depth)}${n.leaf}`, icon: FolderSimple, onSelect: () => moveToDest(ms, { account: n.account, name: n.name, label: n.leaf }) });
    }
    return items;
  };
  const snoozeItems = (ms: MessageSummary[]): MenuItem[] =>
    whenPresets(Math.floor(Date.now() / 1000)).map((p) => ({ label: p.label, onSelect: () => snooze(ms, p.at) }));
  const categoryItems = (ms: MessageSummary[]): MenuItem[] => [
    { label: "Sem categoria", onSelect: () => void applyCategory(ms, null) },
    ...categories.map((c) => ({ label: c.name, onSelect: () => void applyCategory(ms, c.id) })),
  ];

  /** Itens do menu para as mensagens `ms`; `headId` (uma só conversa) liga Responder/Encaminhar/.eml. */
  const menuFor = (ms: MessageSummary[], headId: number | null): MenuItem[] => {
    if (!ms.length) return [];
    const items: MenuItem[] = [];
    if (headId !== null) {
      items.push(
        { label: "Responder", icon: ArrowBendUpLeft, kbd: "Ctrl+R", onSelect: () => void act("reply", headId) },
        { label: "Responder a todos", icon: ArrowBendDoubleUpLeft, kbd: "Ctrl+Shift+R", onSelect: () => void act("replyAll", headId) },
        { label: "Encaminhar", icon: ArrowBendUpRight, kbd: "Ctrl+F", onSelect: () => void act("forward", headId) },
        { separator: true },
      );
    }
    const arch = ms.filter(canArchive);
    items.push(
      { label: arch.length && archiveTo(arch[0]) === "inbox" ? "Mover para a entrada" : "Arquivar", icon: Archive, kbd: "E", disabled: !arch.length, onSelect: () => archive(ms) },
      { label: ms.every((m) => m.role === "trash") ? "Excluir de vez" : "Excluir", icon: Trash, kbd: "Delete", danger: true, disabled: !ms.some(canTrash), onSelect: () => remove(ms) },
      { label: "Relatar phishing", icon: ShieldWarning, disabled: !ms.some(canReport), onSelect: () => reportPhishing(ms) },
      { label: "Mover para", icon: FolderOpen, items: moveItems(ms) },
      ...(() => {
        const head = ms.find((m) => m.id === headId);
        return head?.from_addr ? [
          { label: "Criar regra com este remetente", icon: FunnelSimple, onSelect: () => ruleFromSender(head) },
          { label: "Bloquear remetente", icon: Prohibit, danger: true, onSelect: () => blockSender(head) },
        ] : [];
      })(),
      { separator: true },
      ms.every((m) => m.seen)
        ? { label: "Marcar como não lida", icon: EnvelopeSimple, kbd: "Ctrl+U", onSelect: () => void markSeen(ms, false) }
        : { label: "Marcar como lida", icon: EnvelopeSimpleOpen, kbd: "Ctrl+Q", onSelect: () => void markSeen(ms, true) },
      { label: ms.every((m) => m.flagged) ? "Remover sinalização" : "Sinalizar", icon: Star, kbd: "Insert", onSelect: () => void toggleStar(ms, true) },
      { label: ms.every((m) => m.pinned) ? "Desafixar" : "Fixar no topo", icon: PushPin, kbd: "P", onSelect: () => void togglePin(ms) },
      { label: "Adiar", icon: Clock, items: snoozeItems(ms) },
    );
    if (categories.length) items.push({ label: "Categorias", icon: Tag, items: categoryItems(ms) });
    if (headId !== null) items.push({ separator: true }, { label: "Salvar como .eml", icon: FloppyDisk, onSelect: () => void saveEml(headId) });
    return items;
  };

  const rowContext = (x: number, y: number, row: Row) => {
    const mine = checked.has(row.head.id);
    const ms = mine ? checkedList : withThread(row.members);
    if (!mine && !narrow && selectedId !== row.head.id) void select(row.head.id);
    setMenu({ x, y, items: menuFor(ms, mine && checkedList.length > 1 ? null : row.head.id), label: "Ações da mensagem" });
  };

  const readerContext = (x: number, y: number, selection: string) => {
    if (!current) return;
    const items: MenuItem[] = [];
    if (selection) items.push({ label: "Copiar", icon: Copy, kbd: "Ctrl+C", onSelect: () => { navigator.clipboard?.writeText(selection).catch(() => {}); } }, { separator: true });
    items.push(...menuFor(withThread([current]), current.id));
    if (!ANDROID) items.push({ separator: true }, { label: "Imprimir", icon: Printer, kbd: "Ctrl+P", onSelect: () => window.print() });
    setMenu({ x, y, items, label: "Ações da mensagem" });
  };

  const folderContext = (x: number, y: number, view: View) => {
    const items: MenuItem[] = [
      { label: "Abrir", icon: FolderOpen, onSelect: () => openFolder(view) },
      { label: favorites.includes(view) ? "Remover dos Favoritos" : "Adicionar aos Favoritos", icon: Star, onSelect: () => toggleFavorite(view) },
    ];
    if (!LOCAL_VIEWS.includes(view) && !parseCat(view)) {
      items.push({ label: "Sincronizar", icon: ArrowsClockwise, onSelect: () => {
        syncFolder(view, view === folder ? keep : KEEP).then((ok) => { if (ok && view === folder) return reload(listLen.current); }).catch((e) => toast(`Falha ao sincronizar: ${e}`, "error"));
      } });
    }
    if (parseBox(view)) {
      const node = tree.find((n) => boxView(n.account, n.name) === view);
      if (node) {
        items.push(
          { separator: true },
          { label: "Nova subpasta", icon: FolderSimple, onSelect: () => setFolderDialog({ mode: "create", account: node.account, parent: node.name }) },
          { label: "Renomear", icon: NotePencil, onSelect: () => setFolderDialog({ mode: "rename", node }) },
          { label: "Excluir pasta", icon: Trash, danger: true, onSelect: () => void askDeleteFolder(node) },
        );
      }
    }
    setMenu({ x, y, items, label: "Ações da pasta" });
  };

  function openContact(e: React.MouseEvent<HTMLElement>, raw: string) {
    const r = e.currentTarget.getBoundingClientRect();
    setContact({ x: r.left, y: r.bottom + 4, ...parseContact(raw) });
  }

  // ---- Atalhos (estilo Outlook + teclas simples) ----
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (dialog || compose || settingsOpen || update || folderDialog || searchDialog || menu || contact) return;
      if (folderDelete) {
        if (e.key === "Escape" && !folderBusy) setFolderDelete(null);
        return;
      }
      if (confirmPhish) {
        if (e.key === "Escape") setConfirmPhish(null);
        return;
      }
      if (confirmDelete) {
        if (e.key === "Escape" && !deleting) setConfirmDelete(null);
        return;
      }
      const t = e.target;
      if (ownsKeys(t)) return;
      const typing = isTyping(t);
      if (e.key === "Escape") {
        if (full && !typing) setFull(false);
        else if (typing) { setQuery(""); searchRef.current?.blur(); }
        else if (checked.size) setChecked(new Set());
        else setReaderOpen(false);
        return;
      }
      const action: ShortcutAction | null = shortcutOf(e, typing);
      if (!action) return;
      if ((showDrafts || showScheduled || showOutbox) && action !== "compose" && action !== "search") return;
      if (action === "open" && !(t instanceof Element && t.closest("[role=option]"))) return;
      // Setas/Home/End só movem a seleção com o foco na lista ou fora de qualquer controle (não roubam a rolagem do leitor nem botões).
      if (e.key.startsWith("Arrow") || e.key === "Home" || e.key === "End") {
        const free = !(t instanceof Element) || t === document.body || t === document.documentElement;
        if (!free && !(t instanceof Element && t.closest("[role=listbox]"))) return;
      }
      e.preventDefault();
      const idx = rows.findIndex((r) => r.members.some((m) => m.id === selectedId));
      const targets = checkedList.length ? checkedList : current ? withThread([current]) : [];
      const go = (i: number) => {
        const r = rows[Math.max(0, Math.min(i, rows.length - 1))];
        if (!r) return;
        void select(r.head.id, true);
        listRef.current?.reveal(r.head.id, true);
      };
      switch (action) {
        case "next": return go(idx < 0 ? 0 : idx + 1);
        case "prev": return go(idx < 0 ? 0 : idx - 1);
        case "first": return go(0);
        case "last": return go(rows.length - 1);
        case "open": return setFull(true);
        case "search": searchRef.current?.focus(); searchRef.current?.select(); return;
        case "reply": return void act("reply");
        case "replyAll": return void act("replyAll");
        case "forward": return void act("forward");
        case "compose": return writeNew();
        case "archive": return archive(targets);
        case "delete": return remove(targets);
        case "star": return void toggleStar(targets, true);
        case "read": return void markSeen(targets, true);
        case "unread": return void markSeen(targets, false);
        case "pin": return void togglePin(targets);
        case "undo":
          return void popUndo().then((label) => toast(label ? `Desfeito: ${label}` : "Nada para desfazer", "info", { duration: 2000 }))
            .catch(() => toast("Não foi possível desfazer", "error"));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const empty = accounts !== null && accounts.length === 0;
  /** Conversa com mais de uma mensagem aberta no painel (o fio carrega logo após a seleção). */
  const threaded = grouped && !!detail && thread.length > 1 && thread.some((t) => t.id === detail.id);

  const targets = checkedList.length ? checkedList : current ? withThread([current]) : [];
  const cmds = buildCommands({
    targets, current, hasCategories: categories.length > 0, canArchive, canTrash, archiveTo, writeNew, act, remove, archive, markSeen,
    toggleStar, togglePin, moveItems, snoozeItems, categoryItems, saveEml, reportPhishing, canReport,
  });
  /** Barra de ações do leitor: só no celular (no desktop quem cuida disso é a faixa de comandos). */
  const mobileActions = (
    <MobileActions current={current} categories={categories} canArchive={!!current && canArchive(current)} canTrash={!!current && canTrash(current)}
      toInbox={!!current && archiveTo(current) === "inbox"} inTrash={current?.role === "trash"} popped={!!current && popId === current.id}
      onReply={() => void act("reply")} onReplyAll={() => void act("replyAll")} onForward={() => void act("forward")}
      onArchive={() => current && archive(withThread([current]))} onTrash={() => current && remove(withThread([current]))}
      onStar={() => current && toggleStar(withThread([current]))} onUnread={() => current && markSeen(withThread([current]), false)}
      onPin={() => current && togglePin(withThread([current]))} onCategory={(id) => current && applyCategory(withThread([current]), id)}
      onSnooze={(at) => current && snooze(withThread([current]), at)} />
  );

  const inList = !showDrafts && !showScheduled;
  /** Rascunhos, Agendados e Caixa de saída são de todas as contas: abrem sem conta ativa. */
  const inAll = (run: () => void) => () => { setAccountFilter(null); run(); };
  /** A mesma Rail serve de barra lateral (desktop) e de gaveta (celular). */
  const rail = (variant?: "drawer") => (
    <Rail variant={variant} onNavigate={variant ? drawer.hide : undefined}
      navFolders={folderList} favorites={favorites} folder={folder} inList={inList}
      unread={(v) => (Object.hasOwn(FOLDERS, v) ? (unreadOf(v as Role | "starred") ?? 0) : 0)}
      unreadIn={(account, v) => (SHOWS_UNREAD.includes(v as Role) ? acctCounts[account]?.[v as Role] ?? 0 : 0)}
      open={navigate} drop={dropProps} openMap={openMap} onToggleFolder={toggleFolder} folderUnread={folderUnread} hoverView={dropKey}
      onContext={folderContext} userFolders={userFolders} categories={categories} tree={fullTree}
      searches={savedSearches} activeSearch={activeSearch} openSearch={openSearch} onSearchContext={searchContext} onDeleteSearch={deleteSearch}
      onRenameSearch={(s) => setSearchDialog({ mode: "rename", id: s.id, query: s.query, name: s.name })} accounts={accounts}
      accountFilter={accountFilter} acctLabel={acctLabel} acctColor={acctColor}
      openAccounts={openAccounts} onToggleAccount={(id, open) => setOpenAccounts(open ? (oneAtATime ? [id] : [...openAccounts, id]) : openAccounts.filter((a) => a !== id))}
      drafts={drafts.length} scheduled={scheduled.length} outbox={net.outbox} showDrafts={showDrafts} showScheduled={showScheduled} showOutbox={showOutbox}
      openDrafts={inAll(openDrafts)} openScheduled={inAll(openScheduled)} openOutbox={inAll(openOutbox)}
      onNewFolder={(parent, account) => setFolderDialog({ mode: "create", account: parent?.account ?? account ?? accountFilter ?? undefined, parent: parent?.name })}
      onRenameFolder={(node) => setFolderDialog({ mode: "rename", node })} onDeleteFolder={askDeleteFolder}
      onAddAccount={() => setDialog(true)} />
  );
  /** Antes do título da lista: `Apelido ›` ou `Todas as contas ›` (só com várias contas). */
  const listScopeId = showDrafts || showScheduled ? accountFilter : scopeAccount;
  const listScope = multi ? (listScopeId ? { label: acctLabel(listScopeId), color: acctColor(listScopeId) } : { label: "Todas as contas" }) : undefined;
  const pickTab = (t: "focused" | "other") => changeSetting("inbox:tab", t);
  const pull = usePullToRefresh(syncing, narrow && accounts?.length ? () => void sync(true) : undefined);

  return (
    <AppShell shellRef={appRef} data-reader={readerOpen || undefined} data-pane={pane === "right" ? undefined : pane}
      data-full={full || undefined} data-page={settingsOpen ? "settings" : undefined} data-ready={prefsReady || undefined}
      data-density={density} data-preview={settings.preview === "0" ? "0" : undefined} style={layoutVars}>
      <TopBar query={query} onQuery={setQuery} searchRef={searchRef} folderLabel={info.label} scope={scope}
        canScopeAll={(accounts?.length ?? 0) > 1} onScope={setScope} onFilter={(token) => setQuery(addFilter(query, token))}
        onSaveSearch={() => setSearchDialog({ mode: "create", query, name: suggestName(query) })}
        net={net} syncing={syncing} canSync={!!accounts?.length} onSync={() => void sync(true)}
        onMenu={drawer.show} menuRef={menuRef} menuOpen={drawer.open}
        onAddAccount={() => setDialog(true)} onSettings={() => setSettingsOpen(true)} />
      <CommandBar cmds={cmds} hidden={hiddenTools(settings.toolbarHidden)}
        size={settings.toolbar === "s" || settings.toolbar === "l" ? settings.toolbar : "m"}
        labels={settings.toolbarLabels === "always" || settings.toolbarLabels === "icons" ? settings.toolbarLabels : "auto"} />

      <RouteLayout>
        {rail()}

        <PageLayout area="list" aria-label="Lista de mensagens" {...pull.bind}>
          <ListHead scope={listScope} title={showScheduled ? "Agendados" : showOutbox ? "Caixa de saída" : showDrafts ? "Rascunhos" : info.label}
            tabsOn={tabsOn && inList} tab={tab} unread={tabUnread} onTab={pickTab}>{pull.indicator}</ListHead>
          {checkedList.length > 0 && (
            <BulkBar rows={rows} checkedRows={checkedRows} checked={checkedList} categories={categories}
              canArchive={canArchive} canTrash={canTrash} onClear={() => setChecked(new Set())}
              onAll={() => setChecked(new Set(rows.map((r) => r.head.id)))}
              archive={archive} remove={remove} markSeen={markSeen} toggleStar={toggleStar} snooze={snooze}
              togglePin={togglePin} applyCategory={applyCategory} />
          )}
          {results && inList && (
            <div className="result-meta">
              <span>{plural(results.length, "resultado", "resultados")} para “{query.trim()}”{fromServer && " (com o servidor)"}</span>
              {!fromServer && folder !== "starred" && !String(folder).startsWith("category") && (
                <button type="button" className="btn small" onClick={searchServer} disabled={serverBusy}>{serverBusy ? "Buscando…" : "Buscar no servidor"}</button>
              )}
            </div>
          )}

          {showScheduled ? (
            <ScheduledList items={scheduled} onEdit={editScheduled} onCancel={cancelScheduled} />
          ) : showOutbox ? (
            <OutboxPane items={outbox} accounts={(accounts ?? []).map((a) => a.id)} online={net.online} onChange={refreshOutbox} />
          ) : showDrafts ? (
            <DraftList items={draftsShown} onOpen={openDraft} />
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
            <div className="empty quiet"><p>{loadingFolder ? "Carregando…" : tabsOn ? (tab === "focused" ? "Nada em Foco. Veja a aba Outros." : "Nada em Outros.") : info.empty}</p></div>
          ) : (
            <MessageList ref={listRef} rows={rows} grouped={grouped} density={density} selectedId={selectedId} checked={checked}
              fresh={freshIds} dragIds={dragIds} popId={popId} showAcct={multi && (!scopeAccount || searchAll)} acctLabel={acctLabel} acctColor={acctColor} categories={categories}
              intro={intro} byDate={!results} label="Mensagens" canArchive={canArchive} canTrash={canTrash}
              onOpen={(e, m) => { if (!justSwiped()) onRowClick(e, m); }} onCheck={toggleCheck} onStar={(row) => toggleStar(row.members)}
              onContext={rowContext}
              onDragStart={startRowDrag} onDouble={() => setFull(true)} onNearEnd={() => void loadMore()} onScrollTop={fab.onScroll}
              swipe={(row) => swipeProps(swipeAction, (dir) => swipeRow(row, dir))}
              tools={{
                archive: (row) => archive(withThread(row.members)),
                remove: (row) => remove(withThread(row.members)),
                toggleRead: (row) => void markSeen(withThread(row.members), row.members.some((x) => !x.seen)),
                pin: (row) => void togglePin(withThread(row.members)),
              }} />
          )}
        </PageLayout>

        {!full && !settingsOpen && (
          <div className="resizers" role="region" aria-label="Redimensionar painéis">
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
          </div>
        )}

        <ReaderPane detail={detail && detail.id === selectedId ? detail : null} error={openError && openError.id === selectedId ? openError.msg : null} opening={opening !== null && opening === selectedId}
          onRetry={() => selectedId !== null && void select(selectedId)} selectedId={selectedId} threaded={threaded} thread={thread} bodies={bodies}
          expanded={expanded} full={full} viaKeyboard={viaKeyboard} folderLabel={info.label} hasMessages={messages.length > 0} imagesOk={imagesOk} imagesFrom={imagesFrom}
          savingAtt={savingAtt} mobileActions={mobileActions} onFull={setFull} onBack={() => (full ? setFull(false) : setReaderOpen(false))}
          onContact={openContact} onReaderContext={readerContext} onToggleThread={(id) => void toggleThreadMsg(id)}
          onShowImages={(id) => setImagesOk((s) => new Set(s).add(id))} onTrustSender={trustSender} onReportPhishing={(d) => reportPhishing([d])} onSaveAttachment={(id, att) => void saveAttachment(id, att)} onDragAttachment={dragAttachment}>
          {confirmPhish && (
            <div className="confirm scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) setConfirmPhish(null); }}>
              <div className="confirm-card" role="alertdialog" aria-labelledby="phish-title">
                <p id="phish-title">{confirmPhish.length === 1 ? "Relatar como phishing?" : `Relatar ${confirmPhish.length} mensagens como phishing?`}</p>
                <span>Vão para o Spam. Dá para desfazer logo depois.</span>
                <div className="confirm-actions">
                  <button type="button" className="btn" onClick={() => setConfirmPhish(null)} autoFocus>Cancelar</button>
                  <button type="button" className="btn danger" onClick={() => { const l = confirmPhish; setConfirmPhish(null); moveToDest(l, "junk"); }}>Relatar</button>
                </div>
              </div>
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
        </ReaderPane>
      </RouteLayout>
      <NavDrawer open={drawer.open} onClose={drawer.hide} returnFocus={menuRef}>{() => rail("drawer")}</NavDrawer>

      {dialog && (
        <AccountDialog
          onClose={() => setDialog(false)}
          onSetting={changeSetting}
          onAdded={(a) => {
            const isNew = !(accounts ?? []).some((x) => x.id === a.id);
            setAccounts((l) => [...(l ?? []).filter((x) => x.id !== a.id), a]);
            if (!isNew) return;
            if (settings.notify !== "0") api.requestNotifyPermission().catch(() => {});
            toast(`Conta ${a.username} conectada`, "success");
          }}
          onDone={(a) => {
            setSettingsOpen(false);
            navigate("inbox", a.id);
          }}
        />
      )}
      {compose && accounts && (
        <Composer key={composeKey} init={compose} accounts={accounts} settings={settings} onSend={send} onSchedule={schedule} templates={templates} onDiscard={() => setCompose(null)} />
      )}
      {settingsOpen && (
        <SettingsPage initialSection={settingsSection} accounts={accounts ?? []} settings={settings} onSetting={changeSetting} onRemoveAccount={removeAccount}
          onAddAccount={() => setDialog(true)} onAccountSaved={accountSaved} onClose={() => { setSettingsOpen(false); setSettingsSection(undefined); }} onUpdate={showUpdate} onApplyRules={applyRules} accentAdjusted={accentAdjusted} onPreview={previewSetting} />
      )}
      {folderDialog && (
        <FolderDialog state={folderDialog} accounts={accounts ?? []} folders={userFolders} onSubmit={submitFolder} onClose={() => setFolderDialog(null)} />
      )}
      {searchDialog && <SearchDialog state={searchDialog} onSubmit={submitSearch} onClose={() => setSearchDialog(null)} />}
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
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
      {contact && (
        <ContactCard c={contact} onClose={() => setContact(null)} onNew={(addr) => writeNew(addr)}
          onSearch={(addr) => { setQuery(`de:${addr}`); if (narrow) setReaderOpen(false); }} />
      )}
      {!compose && !dialog && !settingsOpen && !update && (
        <Fab fabRef={fab.ref} onClick={() => writeNew()} />
      )}
      {ghost && <DragGhost elRef={ghostEl} pos={ghostPos} count={ghost.count} copy={ghost.copy} label={ghost.label} />}
      <Toaster />
      <footer className="statusbar"><div className="version" aria-hidden>v{build.version} · {build.builtAt}</div></footer>
    </AppShell>
  );
}
