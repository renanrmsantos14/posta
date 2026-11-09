// Painel de pastas: Favoritos, pastas da conta, pastas do usuário e categorias, rascunhos/agendados e contas.
import { FileText, PaperPlaneTilt, Plus, Clock } from "@phosphor-icons/react";
import type { Account, Category, UserFolder, View } from "./api";
import { FolderTree } from "./FolderTree";
import { PageLayout } from "./layout/primitives";
import { viewInfo, type FolderNode } from "./folders";

type NavView = "all" | "inbox" | "starred" | "sent" | "archive" | "trash" | "junk";

interface Props {
  navFolders: NavView[];
  favorites: View[];
  folder: View;
  inList: boolean;
  unread: (view: View) => number;
  open: (view: View) => void;
  drop: (key: string, view: View) => Record<string, unknown>;
  onContext: (x: number, y: number, view: View) => void;
  userFolders: UserFolder[];
  categories: Category[];
  tree: FolderNode[];
  accounts: Account[] | null;
  accountFilter: string | null;
  onAccountFilter: (id: string | null) => void;
  drafts: number;
  scheduled: number;
  outbox: number;
  showDrafts: boolean;
  showScheduled: boolean;
  showOutbox: boolean;
  openDrafts: () => void;
  openScheduled: () => void;
  openOutbox: () => void;
  onNewFolder: (parent?: FolderNode) => void;
  onRenameFolder: (n: FolderNode) => void;
  onDeleteFolder: (n: FolderNode) => void;
  onAddAccount: () => void;
}

export function Rail(p: Props) {
  const here = (v: View) => (v === p.folder && p.inList ? "page" : undefined);
  const item = (view: View, key = view as string) => {
    const { label, icon: Icon } = viewInfo(view, p.userFolders, p.categories);
    const n = p.unread(view);
    return (
      <button key={key} type="button" className="nav-item" title={label} aria-label={n > 0 ? `${label} ${n} não lidas` : label} aria-current={here(view)} onClick={() => p.open(view)}
        onContextMenu={(e) => { e.preventDefault(); p.onContext(e.clientX, e.clientY, view); }}
        {...(view === "starred" || view === "all" ? {} : p.drop(view, view))}>
        <span className="nav-icon"><Icon size={16} weight="bold" />{n > 0 && <span className="count" aria-hidden>{n}</span>}</span>
        <span className="nav-label">{label}</span>
      </button>
    );
  };
  const extra = (label: string, Icon: typeof FileText, n: number, current: boolean, run: () => void) => (
    <button type="button" className="nav-item" title={label} aria-label={n > 0 ? `${label} ${n}` : label} onClick={run} aria-current={current ? "page" : undefined}>
      <span className="nav-icon"><Icon size={16} weight="bold" />{n > 0 && <span className="count plain" aria-hidden>{n}</span>}</span>
      <span className="nav-label">{label}</span>
    </button>
  );

  return (
    <PageLayout area="rail" as="aside" aria-label="Pastas e contas">
      {p.favorites.length > 0 && (
        <nav className="rail-section" aria-label="Favoritos">
          <div className="rail-label">Favoritos</div>
          {p.favorites.map((v) => item(v, `fav:${v}`))}
        </nav>
      )}
      <nav className="rail-section" aria-label="Caixa de correio">
        <div className="rail-label">Caixa de correio</div>
        {p.navFolders.map((r) => item(r))}
        {extra("Rascunhos", FileText, p.drafts, p.showDrafts && !p.showOutbox, p.openDrafts)}
        {extra("Agendados", Clock, p.scheduled, p.showScheduled, p.openScheduled)}
        {(p.outbox > 0 || p.showOutbox) && extra("Caixa de saída", PaperPlaneTilt, p.outbox, p.showOutbox, p.openOutbox)}
      </nav>
      <FolderTree nodes={p.tree} accounts={p.accounts ?? []} byAccount={(p.accounts?.length ?? 0) > 1 && !p.accountFilter}
        categories={p.categories} folder={p.folder} inList={p.inList} open={p.open} drop={p.drop} onContext={p.onContext}
        onNew={p.onNewFolder} onRename={p.onRenameFolder} onDelete={p.onDeleteFolder} />
      <div className="rail-section" aria-label="Contas">
        <div className="rail-label">Contas</div>
        {p.accounts && p.accounts.length > 1 && (
          <button type="button" className="account account-btn" title="Todas as caixas juntas" aria-pressed={!p.accountFilter} onClick={() => p.onAccountFilter(null)}>
            <span className="avatar" aria-hidden>*</span>
            <span className="account-name">Todas as contas</span>
          </button>
        )}
        {p.accounts?.map((a) => p.accounts!.length > 1 ? (
          <button key={a.id} type="button" className="account account-btn" title={a.id} aria-pressed={p.accountFilter === a.id}
            onClick={() => p.onAccountFilter(p.accountFilter === a.id ? null : a.id)}>
            <span className="avatar" aria-hidden>{a.username[0]?.toUpperCase()}</span>
            <span className="account-name">{a.username}</span>
          </button>
        ) : (
          <div key={a.id} className="account" title={a.id}>
            <span className="avatar" aria-hidden>{a.username[0]?.toUpperCase()}</span>
            <span className="account-name">{a.username}</span>
          </div>
        ))}
        <button type="button" className="btn ghost small add-account" aria-label="Adicionar conta" onClick={p.onAddAccount}>
          <Plus size={14} weight="bold" /> <span>Adicionar conta</span>
        </button>
      </div>
    </PageLayout>
  );
}
