// Barra lateral: pastas do usuário (com subpastas, ações e alvo de soltar) e categorias.
import { Fragment } from "react";
import { FolderSimple, PencilSimple, Plus, Trash } from "@phosphor-icons/react";
import type { Account, Category, View } from "./api";
import type { FolderNode } from "./folders";
import { boxView, catView } from "./views";
import { CategoryDot } from "./Categories";

export type DropProps = Record<string, unknown>;

interface Props {
  nodes: FolderNode[];
  accounts: Account[];
  /** Mostra o nome da conta acima das pastas dela (várias contas, sem filtro). */
  byAccount: boolean;
  categories: Category[];
  folder: View;
  inList: boolean;
  open: (view: View) => void;
  /** Props de arrastar e soltar para o alvo `key`; vazio quando soltar ali não faz sentido. */
  drop: (key: string, view: View) => DropProps;
  onContext?: (x: number, y: number, view: View) => void;
  onNew: (parent?: FolderNode) => void;
  onRename: (node: FolderNode) => void;
  onDelete: (node: FolderNode) => void;
}

export function FolderTree({ nodes, accounts, byAccount, categories, folder, inList, open, drop, onContext, onNew, onRename, onDelete }: Props) {
  return (
    <>
      <div className="rail-section" aria-label="Pastas">
        <div className="rail-head">
          <div className="rail-label">Pastas</div>
          <button type="button" className="icon-btn" aria-label="Nova pasta" title="Nova pasta" onClick={() => onNew()}>
            <Plus size={13} weight="bold" />
          </button>
        </div>
        {nodes.length === 0 && <p className="rail-sub">Nenhuma pasta ainda</p>}
        {nodes.map((n, i) => {
          const view = boxView(n.account, n.name);
          return (
            <Fragment key={`${n.account}/${n.name}`}>
              {byAccount && n.account !== nodes[i - 1]?.account && (
                <div className="rail-sub">{accounts.find((a) => a.id === n.account)?.username ?? n.account}</div>
              )}
              <div className="folder-row" style={{ "--depth": n.depth } as React.CSSProperties}>
                <button type="button" className="nav-item" title={n.name} onClick={() => open(view)}
                  aria-current={view === folder && inList ? "page" : undefined} {...drop(view, view)}
                  onContextMenu={onContext ? (e) => { e.preventDefault(); onContext(e.clientX, e.clientY, view); } : undefined}>
                  <FolderSimple size={16} weight="bold" />
                  <span>{n.label}</span>
                </button>
                <span className="folder-actions">
                  <button type="button" className="icon-btn" aria-label={`Nova subpasta em ${n.leaf}`} title="Nova subpasta" onClick={() => onNew(n)}>
                    <Plus size={12} weight="bold" />
                  </button>
                  <button type="button" className="icon-btn" aria-label={`Renomear ${n.leaf}`} title="Renomear" onClick={() => onRename(n)}>
                    <PencilSimple size={12} weight="bold" />
                  </button>
                  <button type="button" className="icon-btn" aria-label={`Excluir ${n.leaf}`} title="Excluir" onClick={() => onDelete(n)}>
                    <Trash size={12} weight="bold" />
                  </button>
                </span>
              </div>
            </Fragment>
          );
        })}
      </div>

      {categories.length > 0 && (
        <div className="rail-section" aria-label="Categorias">
          <div className="rail-label">Categorias</div>
          {categories.map((c) => {
            const view = catView(c.id);
            return (
              <button key={c.id} type="button" className="nav-item" title={c.name} onClick={() => open(view)}
                aria-current={view === folder && inList ? "page" : undefined}
                onContextMenu={onContext ? (e) => { e.preventDefault(); onContext(e.clientX, e.clientY, view); } : undefined}>
                <CategoryDot color={c.color} />
                <span>{c.name}</span>
              </button>
            );
          })}
        </div>
      )}
    </>
  );
}
