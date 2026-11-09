import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowBendUpLeft, ArrowBendDoubleUpLeft, ArrowBendUpRight, ArrowsClockwise, Archive, Check, MagnifyingGlass, NotePencil, Plus, Star, Trash, Tray, EnvelopeSimple, EnvelopeSimpleOpen, GearSix, Paperclip, X } from "@phosphor-icons/react";
import { HtmlBody, hasRemoteImages } from "./HtmlBody";
import { api, isTauri, type Account, type Attachment, type MessageDetail, type MessageSummary, type OutAttachment, type Role, type UnreadCounts, type View } from "./api";
import { AccountDialog } from "./AccountDialog";
import { Composer, type ComposeInit } from "./Composer";
import { Toaster, toast } from "./toast";
import { FOLDERS, SHOWS_UNREAD, withStarred } from "./folders";
import { fmtSize } from "./format";
import { SettingsDialog } from "./SettingsDialog";
import { UpdateDialog } from "./UpdateDialog";
import { autoCheck, checkUpdate, CHECK_EVERY_MS, skip, skipped, type Update } from "./updater";
import build from "./version.json";

const PAGE = 100;
// Rede de segurança: a chegada normal vem do IDLE (evento `mail-changed`).
const SYNC_EVERY_MS = 300_000;
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

/** Corpo do encaminhamento: cabeçalho da original e o texto citado. */
function forwardBody(d: MessageDetail) {
  const quoted = d.body_text.trimEnd().split("\n").map((l) => `> ${l}`).join("\n");
  const head = [
    "---------- Mensagem encaminhada ----------",
    `De: ${d.from_name ? `${d.from_name} <${d.from_addr}>` : d.from_addr}`,
    `Data: ${fmtLong.format(new Date(d.date * 1000))}`,
    `Assunto: ${d.subject || "(sem assunto)"}`,
    d.to_addrs && `Para: ${d.to_addrs}`,
    d.cc_addrs && `Cc: ${d.cc_addrs}`,
  ].filter(Boolean).join("\n");
  return `\n\n${head}\n\n${quoted}\n`;
}

/** Para onde "Arquivar" leva: de Lixeira/Spam volta à entrada. */
const archiveTo = (m: MessageSummary): Role => (m.role === "trash" || m.role === "junk" ? "inbox" : "archive");

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
  /** Mensagens cujas imagens remotas o usuário liberou (só nesta sessão). */
  const [imagesOk, setImagesOk] = useState<Set<number>>(new Set());
  const [syncing, setSyncing] = useState(false);
  const [dialog, setDialog] = useState(false);
  const [readerOpen, setReaderOpen] = useState(false);
  const [compose, setCompose] = useState<ComposeInit | null>(null);
  const [savingAtt, setSavingAtt] = useState<string | null>(null);
  const [settings, setSettings] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  /** Remonta o painel a cada abertura, mesmo com o mesmo rascunho. */
  const [composeKey, setComposeKey] = useState(0);
  /** Ids que chegaram depois da primeira carga: entram com destaque. */
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  /** Cascata só na primeira abertura; buscas e atualizações trocam a lista sem animar. */
  const [intro, setIntro] = useState(true);
  const [folder, setFolder] = useState<View>("inbox");
  const [folderList, setFolderList] = useState<View[]>(["inbox", "starred"]);
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
  const checkedList = useMemo(() => visible.filter((m) => checked.has(m.id)), [visible, checked]);
  /** Só a pasta real sincroniza com o servidor; "Com estrela" é uma visão do banco local. */
  const remote = folder !== "inbox" && folder !== "starred";

  const current = visible.find((m) => m.id === selectedId) ?? null;
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
      if (manual && remote) {
        await syncFolder(folder, keep);
        await reload(messages.length);
        return;
      }
      const r = await api.sync();
      if (r.skipped) return;
      r.errors.forEach((e) => toast(e, "error"));
      api.folders().then((r) => setFolderList(withStarred(r))).catch(() => {});
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

  useEffect(() => {
    api.listAccounts().then(setAccounts);
    api.folders().then((r) => setFolderList(withStarred(r))).catch(() => {});
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
        if (alive && remote) {
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
    if (folderRef.current !== "inbox" && folderRef.current !== "starred") return;
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

  // Busca local a cada tecla: sem debounce, o SQLite responde em < 1 ms.
  useEffect(() => {
    if (!query.trim()) return setResults(null);
    let alive = true;
    api.search(query, folder).then((r) => alive && setResults(r));
    return () => { alive = false; };
  }, [query, folder]);

  function openFolder(role: View) {
    if (role === folder) return;
    setQuery("");
    setResults(null);
    setMessages([]);
    setSelectedId(null);
    setDetail(null);
    setChecked(new Set());
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
      refreshCounts();
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

  /** Remetente em Para; os demais destinatários da original em Cc, sem as minhas contas e sem repetir. */
  function replyAll() {
    if (!detail || !accounts?.length) return;
    const own = new Set(accounts.map((a) => a.username.toLowerCase()));
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
      to: detail.reply_addr, cc: others.join(", "), subject, body: replyBody(detail),
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
    openComposer({ title: "Encaminhar", account, reply_to: null, to: "", subject, body: forwardBody(detail), attachments });
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
      } else if (remote && serverTotal !== null && serverTotal > keep) {
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
  function moveOut(ms: MessageSummary[], to: Role) {
    const ids = ms.map((m) => m.id);
    const undo = takeOutMany(ms);
    ids.forEach((id) => pendingMove.current.add(id));
    const restore = () => { ids.forEach((id) => pendingMove.current.delete(id)); undo(); };
    const text = to === "trash" ? "Movida para a Lixeira" : to === "inbox" ? "Movida para a Caixa de entrada" : "Arquivada";
    toast(ms.length === 1 ? text : `${plural(ms.length, "mensagem movida", "mensagens movidas")}`, "info", {
      action: { label: "Desfazer", onClick: restore },
      duration: UNDO_MS,
      onClose: async (reason) => {
        if (reason === "action") return;
        try {
          await api.moveMany(ids, to);
          ids.forEach((id) => pendingMove.current.delete(id));
          refreshCounts();
        } catch (e) {
          restore();
          toast(`Não foi possível mover: ${e}`, "error");
        }
      },
    });
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
    const from = visible.findIndex((x) => x.id === (anchor.current ?? selectedId));
    const to = visible.findIndex((x) => x.id === m.id);
    setChecked((c) => {
      const next = new Set(c);
      if (range && from >= 0) {
        visible.slice(Math.min(from, to), Math.max(from, to) + 1).forEach((x) => next.add(x.id));
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
      if (dialog || compose || settings || update) return;
      if (confirmDelete) {
        if (e.key === "Escape" && !deleting) setConfirmDelete(null);
        return;
      }
      const t = e.target;
      const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
      if (e.key === "Escape") {
        if (typing) { setQuery(""); searchRef.current?.blur(); }
        else if (checked.size) setChecked(new Set());
        else setReaderOpen(false);
        return;
      }
      if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
      const idx = visible.findIndex((m) => m.id === selectedId);
      const acts = checkedList.length ? checkedList : current ? [current] : [];
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
                {r !== "starred" && SHOWS_UNREAD.includes(r) && !!counts[r] && <span className="count">{counts[r]}</span>}
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
          <span><kbd>a</kbd> responder a todos</span>
          <span><kbd>f</kbd> encaminhar</span>
          <span><kbd>c</kbd> escrever</span>
          <span><kbd>e</kbd> arquivar</span>
          <span><kbd>#</kbd> excluir</span>
          <span><kbd>s</kbd> estrela</span>
          <span><kbd>u</kbd> não lida</span>
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
        {checkedList.length > 0 && (
          <div className="bulk" role="toolbar" aria-label="Ações em lote">
            <button className="icon-btn" onClick={() => setChecked(new Set())} aria-label="Limpar seleção" title="Limpar seleção (Esc)">
              <X size={15} weight="bold" />
            </button>
            <span className="bulk-count">{checkedList.length} {checkedList.length === 1 ? "selecionada" : "selecionadas"}</span>
            {checkedList.length < visible.length && (
              <button className="btn ghost small" onClick={() => setChecked(new Set(visible.map((m) => m.id)))}>Todas</button>
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
            </span>
          </div>
        )}
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
            data-selecting={checkedList.length > 0 || undefined} aria-multiselectable
            onScroll={(e) => {
              const el = e.currentTarget;
              if (el.scrollHeight - el.scrollTop - el.clientHeight < 400) loadMore();
            }}>
            {visible.map((m, i) => (
              <li key={m.id} role="option" aria-selected={m.id === selectedId}
                className="row" data-unread={!m.seen || undefined} data-fresh={freshIds.has(m.id) || undefined}
                data-checked={checked.has(m.id) || undefined}
                style={{ "--i": Math.min(i, 12) } as React.CSSProperties}
                onClick={(e) => onRowClick(e, m)}>
                <span className="dot" aria-hidden />
                <button type="button" className="check" role="checkbox" aria-checked={checked.has(m.id)}
                  aria-label={`Selecionar mensagem de ${m.from_name || m.from_addr}`}
                  onClick={(e) => { e.stopPropagation(); toggleCheck(m, e.shiftKey); }}>
                  <Check size={11} weight="bold" />
                </button>
                <span className="from">{m.from_name || m.from_addr}</span>
                <time className="date" dateTime={new Date(m.date * 1000).toISOString()}>{shortDate(m.date)}</time>
                <button type="button" className="star" data-on={m.flagged || undefined} data-pop={popId === m.id || undefined}
                  aria-pressed={m.flagged} aria-label={m.flagged ? "Tirar a estrela" : "Marcar com estrela"}
                  onClick={(e) => { e.stopPropagation(); toggleStar([m]); }}>
                  <Star size={15} weight={m.flagged ? "fill" : "bold"} />
                </button>
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
              <button className="btn icon-only" onClick={replyAll} aria-label="Responder a todos" title="Responder a todos (a)">
                <ArrowBendDoubleUpLeft size={15} weight="bold" />
              </button>
              <button className="btn icon-only" onClick={forward} aria-label="Encaminhar" title="Encaminhar (f)">
                <ArrowBendUpRight size={15} weight="bold" />
              </button>
              {current && canArchive(current) && (
                <button className="btn" onClick={() => archive([current])}>
                  {archiveTo(current) === "inbox"
                    ? <><Tray size={15} weight="bold" /> Mover para a entrada</>
                    : <><Archive size={15} weight="bold" /> Arquivar</>} <kbd>e</kbd>
                </button>
              )}
              <button className="btn" onClick={() => current && remove([current])} disabled={!current || !canTrash(current)}
                title={current && !canTrash(current) ? "Esta conta não tem Lixeira" : undefined}>
                <Trash size={15} weight="bold" /> {current?.role === "trash" ? "Excluir de vez" : "Excluir"} <kbd>#</kbd>
              </button>
              {current && (
                <>
                  <button className="btn icon-only star-btn" data-on={current.flagged || undefined} data-pop={popId === current.id || undefined}
                    onClick={() => toggleStar([current])} aria-pressed={current.flagged}
                    aria-label={current.flagged ? "Tirar a estrela" : "Marcar com estrela"} title="Estrela (s)">
                    <Star size={15} weight={current.flagged ? "fill" : "bold"} />
                  </button>
                  <button className="btn icon-only" onClick={() => markSeen([current], false)}
                    aria-label="Marcar como não lida" title="Marcar como não lida (u)">
                    <EnvelopeSimple size={15} weight="bold" />
                  </button>
                </>
              )}
            </div>
            {detail.attachments.length > 0 && (
              <ul className="attachments" aria-label="Anexos">
                {detail.attachments.map((a) => (
                  <li key={a.index}>
                    <button type="button" className="chip attachment" disabled={savingAtt === `${detail.id}:${a.index}`}
                      title={`Salvar e abrir ${a.name}`} onClick={() => void saveAttachment(detail.id, a)}>
                      <Paperclip size={14} weight="bold" aria-hidden />
                      <span className="chip-name">{a.name}</span>
                      <span className="chip-size">{fmtSize(a.size)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {detail.body_html ? (
              <>
                {!imagesOk.has(detail.id) && hasRemoteImages(detail.body_html) && (
                  <div className="img-banner" role="status">
                    <span>Imagens remotas bloqueadas para proteger sua privacidade.</span>
                    <button type="button" className="btn" onClick={() => setImagesOk((s) => new Set(s).add(detail.id))}>
                      Mostrar imagens
                    </button>
                  </div>
                )}
                <HtmlBody html={detail.body_html} remote={imagesOk.has(detail.id)} />
              </>
            ) : (
              <div className="body">{detail.body_text}</div>
            )}
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
            toast(`Conta ${a.username} conectada`, "success");
          }}
        />
      )}
      {compose && accounts && (
        <Composer key={composeKey} init={compose} accounts={accounts} onSend={send} onDiscard={() => setCompose(null)} />
      )}
      {settings && <SettingsDialog onClose={() => setSettings(false)} onUpdate={showUpdate} />}
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
