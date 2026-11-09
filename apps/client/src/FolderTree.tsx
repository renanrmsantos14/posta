// Barra lateral: pastas do usuário (com subpastas, ações e alvo de soltar) e categorias.
import { Fragment, useEffect } from "react";
import { CaretDown, CaretRight, FolderSimple, PencilSimple, Plus, Trash } from "@phosphor-icons/react";
import type { Account, Category, View } from "./api";
import { descendants, folderKey, isFolderOpen, visibleFolders, type FolderNode, type OpenMap } from "./folders";
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
  /** Pastas abertas/fechadas à mão e a troca de uma delas. */
  openMap: OpenMap;
  onToggle: (node: FolderNode, open: boolean) => void;
  /** Não lidas por pasta (chave de `folderKey`). */
  unread: Map<string, number>;
  /** Pasta sob o ponteiro durante um arrasto: se estiver fechada, abre depois de um instante. */
  hoverView: string | null;
}

/** Quanto o ponteiro precisa ficar sobre uma pasta fechada, arrastando, para ela abrir. */
const HOVER_OPEN_MS = 600;

export function FolderTree({ nodes, accounts, byAccount, categories, folder, inList, open, drop, onContext, onNew, onRename, onDelete, openMap, onToggle, unread, hoverView }: Props) {
  useEffect(() => {
    if (!hoverView) return;
    const i = nodes.findIndex((n) => boxView(n.account, n.name) === hoverView);
    if (i < 0 || !descendants(nodes, i) || isFolderOpen(nodes[i], openMap)) return;
    const t = setTimeout(() => onToggle(nodes[i], true), HOVER_OPEN_MS);
    return () => clearTimeout(t);
  }, [hoverView, nodes, openMap, onToggle]);

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
        {visibleFolders(nodes, openMap).map((i) => {
          const n = nodes[i];
          const view = boxView(n.account, n.name);
          const kids = descendants(nodes, i);
          const isOpen = isFolderOpen(n, openMap);
          // Fechada, a pasta mostra o total dela e de tudo que está dentro.
          let count = unread.get(folderKey(n)) ?? 0;
          if (kids && !isOpen) for (let j = i + 1; j <= i + kids; j++) count += unread.get(folderKey(nodes[j])) ?? 0;
          const prev = nodes[i - 1];
          return (
            <Fragment key={folderKey(n)}>
              {byAccount && n.account !== prev?.account && (
                <div className="rail-sub">{accounts.find((a) => a.id === n.account)?.username ?? n.account}</div>
              )}
              <div className="folder-row" style={{ "--depth": n.depth } as React.CSSProperties}>
                {kids > 0 && (
                  <button type="button" className="folder-caret" tabIndex={-1} aria-label={`${isOpen ? "Recolher" : "Expandir"} ${n.leaf}`}
                    title={isOpen ? "Recolher" : "Expandir"} onClick={() => onToggle(n, !isOpen)}>
                    {isOpen ? <CaretDown size={11} weight="bold" /> : <CaretRight size={11} weight="bold" />}
                  </button>
                )}
                <button type="button" className="nav-item" title={n.name} onClick={() => open(view)}
                  aria-current={view === folder && inList ? "page" : undefined} aria-expanded={kids > 0 ? isOpen : undefined} {...drop(view, view)}
                  onDoubleClick={kids > 0 ? () => onToggle(n, !isOpen) : undefined}
                  onKeyDown={kids > 0 ? (e) => {
                    if (e.altKey || e.ctrlKey || e.metaKey) return;
                    if (e.key === "ArrowRight" && !isOpen) { e.preventDefault(); onToggle(n, true); }
                    else if (e.key === "ArrowLeft" && isOpen) { e.preventDefault(); onToggle(n, false); }
                  } : undefined}
                  onContextMenu={onContext ? (e) => { e.preventDefault(); onContext(e.clientX, e.clientY, view); } : undefined}>
                  <FolderSimple size={16} weight="bold" />
                  <span>{n.label}</span>
                  {count > 0 && <span className="count" aria-label={`${count} não lidas`}>{count}</span>}
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
