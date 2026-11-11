// Painel de pastas: Favoritos, "Todas as contas" e, com várias contas, uma seção recolhível por conta (pastas do sistema e do usuário),
// no estilo do Outlook: cada conta abre e fecha por conta própria e o cabeçalho mostra o endereço.
// Com uma conta só, é a lista simples de sempre. O mesmo componente serve de gaveta no celular (`variant="drawer"`).
import { CaretRight, FileText, PaperPlaneTilt, Plus, Clock } from "@phosphor-icons/react";
import type { Account, Category, UserFolder, View } from "./api";
import { FolderTree } from "./FolderTree";
import { PageLayout } from "./layout/primitives";
import { viewInfo, type FolderNode, type OpenMap } from "./folders";
import { parseBox } from "./views";

type NavView = "inbox" | "starred" | "sent" | "archive" | "trash" | "junk";
/** Visões da seção "Todas as contas". */
const ALL_VIEWS: NavView[] = ["inbox", "starred", "sent"];

interface Props {
  navFolders: NavView[];
  favorites: View[];
  folder: View;
  inList: boolean;
  /** Não lidas da visão somando as contas. */
  unread: (view: View) => number;
  /** Não lidas da visão numa conta (0 nas pastas sem contador). */
  unreadIn: (account: string, view: View) => number;
  /** Abre a visão; `account` é a conta ativa (null = todas). */
  open: (view: View, account?: string | null) => void;
  drop: (key: string, view: string) => Record<string, unknown>;
  onContext: (x: number, y: number, view: View) => void;
  userFolders: UserFolder[];
  categories: Category[];
  tree: FolderNode[];
  accounts: Account[] | null;
  /** Conta ativa (null = todas). */
  accountFilter: string | null;
  acctLabel: (id: string) => string;
  acctColor: (id: string) => string;
  /** Seções de conta abertas (várias ao mesmo tempo) e a troca de uma delas. */
  openAccounts: string[];
  onToggleAccount: (id: string, open: boolean) => void;
  drafts: number;
  scheduled: number;
  outbox: number;
  showDrafts: boolean;
  showScheduled: boolean;
  showOutbox: boolean;
  openDrafts: () => void;
  openScheduled: () => void;
  openOutbox: () => void;
  onNewFolder: (parent?: FolderNode, account?: string) => void;
  onRenameFolder: (n: FolderNode) => void;
  onDeleteFolder: (n: FolderNode) => void;
  onAddAccount: () => void;
  openMap: OpenMap;
  onToggleFolder: (n: FolderNode, open: boolean) => void;
  folderUnread: Map<string, number>;
  hoverView: string | null;
  variant?: "rail" | "drawer";
  /** Chamado depois de escolher uma pasta (a gaveta fecha). */
  onNavigate?: () => void;
}

export function Rail(p: Props) {
  const accounts = p.accounts ?? [];
  const multi = accounts.length > 1;
  const drawer = p.variant === "drawer";
  const go = (run: () => void) => { run(); p.onNavigate?.(); };
  /** Conta da visão: pastas do usuário já pertencem a uma conta; as demais seguem a seção onde o item está. */
  const here = (v: View, account: string | null) =>
    v === p.folder && p.inList && (parseBox(v) !== null || p.accountFilter === account) ? "page" : undefined;
  const item = (view: View, account: string | null = null, key: string = view) => {
    const { label, icon: Icon } = viewInfo(view, p.userFolders, p.categories);
    const n = account ? p.unreadIn(account, view) : p.unread(view);
    /** Na seção de uma conta, o alvo de soltar leva a conta (`papel|conta`). */
    const dropAt = account ? `${view}|${account}` : view;
    return (
      <button key={key} type="button" className="nav-item" title={label} aria-label={n > 0 ? `${label} ${n} não lidas` : label} aria-current={here(view, account)}
        onClick={() => go(() => p.open(view, account))}
        onContextMenu={(e) => { e.preventDefault(); p.onContext(e.clientX, e.clientY, view); }}
        {...(view === "starred" ? {} : p.drop(dropAt, dropAt))}>
        <span className="nav-icon"><Icon size={16} weight="bold" /></span>
        <span className="nav-label">{label}</span>
        {n > 0 && <>{" "}<span className="count" aria-hidden>{n}</span></>}
      </button>
    );
  };
  /** Rascunhos, Agendados e Caixa de saída valem para todas as contas: abrem sem conta ativa. */
  const extra = (label: string, Icon: typeof FileText, n: number, current: boolean, run: () => void) => (
    <button type="button" className="nav-item" title={label} aria-label={n > 0 ? `${label} ${n}` : label} onClick={() => go(run)}
      aria-current={current && !(multi && p.accountFilter) ? "page" : undefined}>
      <span className="nav-icon"><Icon size={16} weight="bold" /></span>
      <span className="nav-label">{label}</span>
      {n > 0 && <>{" "}<span className="count plain" aria-hidden>{n}</span></>}
    </button>
  );
  const extras = (
    <>
      {extra("Rascunhos", FileText, p.drafts, p.showDrafts && !p.showOutbox, p.openDrafts)}
      {extra("Agendados", Clock, p.scheduled, p.showScheduled, p.openScheduled)}
      {(p.outbox > 0 || p.showOutbox) && extra("Caixa de saída", PaperPlaneTilt, p.outbox, p.showOutbox, p.openOutbox)}
    </>
  );
  const tree = (nodes: FolderNode[], section?: "folders" | "categories", account?: string) => (
    <FolderTree nodes={nodes} section={section}
      categories={p.categories} folder={p.folder} inList={p.inList} open={(v) => go(() => p.open(v, account ?? null))} drop={p.drop} onContext={p.onContext}
      onNew={(parent) => p.onNewFolder(parent, account)} onRename={p.onRenameFolder} onDelete={p.onDeleteFolder}
      openMap={p.openMap} onToggle={p.onToggleFolder} unread={p.folderUnread} hoverView={p.hoverView} />
  );

  // Várias contas: seção "Todas as contas" e uma seção recolhível por conta.
  const inboxOf = (id: string) => p.unreadIn(id, "inbox");

  const section = (id: string) => {
    const open = p.openAccounts.includes(id);
    const n = inboxOf(id);
    const label = accounts.find((a) => a.id === id)?.username || p.acctLabel(id);
    return (
      <section key={id} className="acct-sec" data-open={open || undefined} data-acct-color={p.acctColor(id)} aria-label={label}>
        <button type="button" className="acct-head" aria-expanded={open} aria-controls={`acct-${p.variant ?? "rail"}-${id}`} title={label}
          aria-label={n > 0 ? `${label} ${n} não lidas` : label} onClick={() => p.onToggleAccount(id, !open)}>
          <CaretRight size={11} weight="bold" className="acct-caret" aria-hidden />
          <span className="acct-dot" aria-hidden />
          <span className="acct-name">{label}</span>
          {n > 0 && <>{" "}<span className="count" aria-hidden>{n}</span></>}
        </button>
        <div className="acct-body" id={`acct-${p.variant ?? "rail"}-${id}`}>
          <div className="acct-inner">
            {p.navFolders.map((r) => item(r, id, `${id}:${r}`))}
            {tree(p.tree.filter((f) => f.account === id), "folders", id)}
          </div>
        </div>
      </section>
    );
  };

  const body = (
    <>
      {p.favorites.length > 0 && (
        <nav className="rail-section" aria-label="Favoritos">
          <div className="rail-label">Favoritos</div>
          {p.favorites.map((v) => item(v, null, `fav:${v}`))}
        </nav>
      )}
      <nav className="rail-section" aria-label={multi ? "Todas as contas" : "Caixa de correio"}>
        <div className="rail-label">{multi ? "Todas as contas" : "Caixa de correio"}</div>
        {(multi ? p.navFolders.filter((r) => ALL_VIEWS.includes(r as NavView)) : p.navFolders).map((r) => item(r))}
        {extras}
      </nav>
      {multi ? (
        <>
          <div className="acct-list">{accounts.map((a) => section(a.id))}</div>
          {tree([], "categories")}
        </>
      ) : tree(p.tree)}
      <div className="rail-section" aria-label={multi ? undefined : "Contas"}>
        {!multi && <div className="rail-label">Contas</div>}
        {!multi && accounts.map((a) => (
          <div key={a.id} className="account" title={a.id}>
            <span className="avatar" data-color={p.acctColor(a.id)} aria-hidden>{p.acctLabel(a.id)[0]?.toUpperCase()}</span>
            <span className="account-name">{a.username}</span>
          </div>
        ))}
        <button type="button" className="btn ghost small add-account" aria-label="Adicionar conta" onClick={() => go(p.onAddAccount)}>
          <Plus size={14} weight="bold" /> <span>Adicionar conta</span>
        </button>
      </div>
    </>
  );

  return drawer
    ? <aside className="rail-panel" aria-label="Pastas e contas">{body}</aside>
    : <PageLayout area="rail" as="aside" aria-label="Pastas e contas">{body}</PageLayout>;
}
