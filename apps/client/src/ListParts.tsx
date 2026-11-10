import { Archive, Clock, EnvelopeSimple, EnvelopeSimpleOpen, FileText, PaperPlaneTilt, PushPin, Star, Trash, X } from "@phosphor-icons/react";
import type { Category, MessageSummary, Role, UserFolder, View } from "./api";
import { CategoryPicker } from "./Categories";
import { WhenMenu } from "./WhenMenu";
import { FOLDERS, viewInfo, type FolderNode } from "./folders";
import { boxView, catView } from "./views";
import type { ListRow } from "./MessageList";

type Tab = "focused" | "other";

/** Grupo de pastas do layout estreito (a Rail some no celular). */
export function FolderTabs(p: {
  navFolders: (Role | "starred")[]; tree: FolderNode[]; categories: Category[]; userFolders: UserFolder[];
  folder: View; inList: boolean; showDrafts: boolean; showScheduled: boolean; showOutbox: boolean; outbox: number;
  onOpen: (v: View) => void; onDrafts: () => void; onScheduled: () => void; onOutbox: () => void;
}) {
  const views = [...p.tree.map((n) => boxView(n.account, n.name)), ...p.categories.map((c) => catView(c.id))];
  return (
    <div className="folder-tabs mobile-only" role="group" aria-label="Pastas">
      {p.navFolders.map((r) => {
        const Icon = FOLDERS[r].icon;
        return (
          <button key={r} className="btn ghost small" onClick={() => p.onOpen(r)} aria-current={r === p.folder && p.inList ? "page" : undefined}>
            <Icon size={14} weight="bold" /> {FOLDERS[r].label}
          </button>
        );
      })}
      {views.map((v) => {
        const { label, icon: Icon } = viewInfo(v, p.userFolders, p.categories);
        return (
          <button key={v} className="btn ghost small" onClick={() => p.onOpen(v)} aria-current={v === p.folder && p.inList ? "page" : undefined}>
            <Icon size={14} weight="bold" /> {label}
          </button>
        );
      })}
      <button className="btn ghost small" onClick={p.onDrafts} aria-current={p.showDrafts && !p.showOutbox ? "page" : undefined}>
        <FileText size={14} weight="bold" /> Rascunhos
      </button>
      <button className="btn ghost small" onClick={p.onScheduled} aria-current={p.showScheduled ? "page" : undefined}>
        <Clock size={14} weight="bold" /> Agendados
      </button>
      {(p.outbox > 0 || p.showOutbox) && (
        <button className="btn ghost small" onClick={p.onOutbox} aria-current={p.showOutbox ? "page" : undefined}>
          <PaperPlaneTilt size={14} weight="bold" /> Caixa de saída
        </button>
      )}
    </div>
  );
}

/** Título da lista e abas Foco/Outros (setas esquerda/direita trocam de aba). */
export function ListHead(p: {
  title: string; /** Conta/escopo antes do título: `Apelido › Pasta`. */ scope?: { label: string; color?: string };
  tabsOn: boolean; tab: Tab; unread: { focused: number; other: number }; onTab: (t: Tab) => void;
}) {
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    p.onTab(p.tab === "focused" ? "other" : "focused");
    requestAnimationFrame(() => document.querySelector<HTMLElement>(".focus-tabs [aria-selected=true]")?.focus());
  };
  return (
    <div className="list-head">
      <h2 className="list-title" data-acct-color={p.scope?.color}>
        {p.scope && (
          <span className="list-scope">
            {p.scope.color && <span className="acct-dot" aria-hidden />}
            {p.scope.label}<span className="sr-only"> › </span>
          </span>
        )}
        {p.title}
      </h2>
      {p.tabsOn && (
        <div className="focus-tabs" role="tablist" aria-label="Foco e Outros" onKeyDown={onKey}>
          <button type="button" role="tab" id="tab-focused" aria-selected={p.tab === "focused"} tabIndex={p.tab === "focused" ? 0 : -1} onClick={() => p.onTab("focused")}>
            Foco{p.unread.focused > 0 && <span className="tab-n">{p.unread.focused}</span>}
          </button>
          <button type="button" role="tab" id="tab-other" aria-selected={p.tab === "other"} tabIndex={p.tab === "other" ? 0 : -1} onClick={() => p.onTab("other")}>
            Outros{p.unread.other > 0 && <span className="tab-n">{p.unread.other}</span>}
          </button>
        </div>
      )}
    </div>
  );
}

/** Barra de ações em lote para as linhas marcadas. */
export function BulkBar(p: {
  rows: ListRow[]; checkedRows: ListRow[]; checked: MessageSummary[]; categories: Category[];
  canArchive: (m: MessageSummary) => boolean; canTrash: (m: MessageSummary) => boolean;
  onClear: () => void; onAll: () => void;
  archive: (m: MessageSummary[]) => void; remove: (m: MessageSummary[]) => void;
  markSeen: (m: MessageSummary[], seen: boolean) => void; toggleStar: (m: MessageSummary[]) => void;
  snooze: (m: MessageSummary[], at: number) => void; togglePin: (m: MessageSummary[]) => void;
  applyCategory: (m: MessageSummary[], id: string | null) => void;
}) {
  const list = p.checked;
  return (
    <div className="bulk" role="toolbar" aria-label="Ações em lote">
      <button className="icon-btn" onClick={p.onClear} aria-label="Limpar seleção" title="Limpar seleção (Esc)">
        <X size={15} weight="bold" />
      </button>
      <span className="bulk-count">{p.checkedRows.length} {p.checkedRows.length === 1 ? "selecionada" : "selecionadas"}</span>
      {p.checkedRows.length < p.rows.length && (
        <button className="btn ghost small" onClick={p.onAll}>Todas</button>
      )}
      <span className="bulk-actions">
        <button className="icon-btn" onClick={() => p.archive(list)} disabled={!list.some(p.canArchive)}
          aria-label="Arquivar" title="Arquivar (E)"><Archive size={16} weight="bold" /></button>
        <button className="icon-btn" onClick={() => p.remove(list)} disabled={!list.some(p.canTrash)}
          aria-label="Excluir" title="Excluir (Delete)"><Trash size={16} weight="bold" /></button>
        <button className="icon-btn" onClick={() => p.markSeen(list, true)}
          aria-label="Marcar como lida" title="Marcar como lida (Ctrl+Q)"><EnvelopeSimpleOpen size={16} weight="bold" /></button>
        <button className="icon-btn" onClick={() => p.markSeen(list, false)}
          aria-label="Marcar como não lida" title="Marcar como não lida (Ctrl+U)"><EnvelopeSimple size={16} weight="bold" /></button>
        <button className="icon-btn" onClick={() => p.toggleStar(list)}
          aria-label="Alternar estrela" title="Sinalizar (Insert)"><Star size={16} weight="bold" /></button>
        <WhenMenu className="icon-btn" label="Adiar" onPick={(at) => p.snooze(list, at)}><Clock size={16} weight="bold" /></WhenMenu>
        <button className="icon-btn" onClick={() => p.togglePin(list)}
          aria-label="Alternar fixar" title="Fixar no topo (P)"><PushPin size={16} weight="bold" /></button>
        <CategoryPicker className="icon-btn" categories={p.categories} onPick={(id) => p.applyCategory(list, id)}
          current={list.every((m) => m.category === list[0].category) ? list[0].category ?? null : undefined} />
      </span>
    </div>
  );
}
