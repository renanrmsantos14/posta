// Barra superior: marca, busca central (com escopo e filtros) e ações globais (sincronizar, conta, configurações).
import { useRef, useState } from "react";
import { ArrowsClockwise, Check, FunnelSimple, GearSix, List, MagnifyingGlass, Plus, X } from "@phosphor-icons/react";
import { OfflineBadge } from "./Offline";
import type { QueueStatus } from "./api";
import { SEARCH_FILTERS } from "./productivity";
import { useDismiss } from "./WhenMenu";

export type SearchScope = "folder" | "all";

interface Props {
  query: string;
  onQuery: (q: string) => void;
  searchRef: React.RefObject<HTMLInputElement | null>;
  folderLabel: string;
  scope: SearchScope;
  /** "Todas as caixas" só existe com 2+ contas. */
  canScopeAll: boolean;
  onScope: (s: SearchScope) => void;
  onFilter: (token: string) => void;
  net: QueueStatus;
  syncing: boolean;
  canSync: boolean;
  onSync: () => void;
  onAddAccount: () => void;
  /** Celular: abre a gaveta de pastas e contas. */
  onMenu: () => void;
  menuRef: React.RefObject<HTMLButtonElement | null>;
  menuOpen: boolean;
  onSettings: () => void;
}

export function TopBar(p: Props) {
  const [menu, setMenu] = useState<"filter" | "scope" | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useDismiss(menu !== null, () => setMenu(null), root);
  const scopeLabel = p.scope === "all" ? "Todas as caixas" : p.folderLabel;

  return (
    <header className="topbar" role="banner">
      <button ref={p.menuRef} type="button" className="icon-btn mobile-only menu-btn" aria-label="Pastas e contas" title="Pastas e contas"
        aria-haspopup="dialog" aria-expanded={p.menuOpen} onClick={p.onMenu}>
        <List size={18} weight="bold" />
      </button>
      <div className="brand">Posta</div>
      <div className="searchbox" ref={root}>
        <label className="search">
          <MagnifyingGlass size={16} weight="bold" aria-hidden />
          <input ref={p.searchRef} type="search" placeholder={`Buscar em ${scopeLabel}`} value={p.query}
            onChange={(e) => p.onQuery(e.target.value)} aria-label="Buscar mensagens" />
          {p.query ? (
            <button type="button" className="search-clear" aria-label="Limpar busca" onClick={() => { p.onQuery(""); p.searchRef.current?.focus(); }}>
              <X size={13} weight="bold" />
            </button>
          ) : <kbd>Ctrl+E</kbd>}
        </label>
        {p.canScopeAll ? (
          <button type="button" className="scope-chip" aria-haspopup="menu" aria-expanded={menu === "scope"}
            aria-label={`Escopo da busca: ${scopeLabel}`} onClick={() => setMenu(menu === "scope" ? null : "scope")}>
            {scopeLabel}
          </button>
        ) : (
          <span className="scope-chip" data-static>{scopeLabel}</span>
        )}
        <button type="button" className="icon-btn" aria-label="Filtros da busca" title="Filtros da busca" aria-haspopup="menu" aria-expanded={menu === "filter"}
          onClick={() => setMenu(menu === "filter" ? null : "filter")}>
          <FunnelSimple size={16} weight="bold" />
        </button>
        {menu === "filter" && (
          <div className="top-menu" role="menu" aria-label="Filtros da busca">
            {SEARCH_FILTERS.map((f) => (
              <button key={f.token} type="button" role="menuitem" onClick={() => { p.onFilter(f.token); setMenu(null); p.searchRef.current?.focus(); }}>
                <span>{f.label}</span><small>{f.token}</small>
              </button>
            ))}
          </div>
        )}
        {menu === "scope" && (
          <div className="top-menu" data-at="scope" role="menu" aria-label="Escopo da busca">
            {([["folder", p.folderLabel], ["all", "Todas as caixas"]] as const).map(([key, label]) => (
              <button key={key} type="button" role="menuitemradio" aria-checked={p.scope === key} onClick={() => { p.onScope(key); setMenu(null); p.searchRef.current?.focus(); }}>
                <span>{label}</span>{p.scope === key && <Check size={13} weight="bold" aria-hidden />}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="top-actions">
        <OfflineBadge status={p.net} />
        <button type="button" className="icon-btn" onClick={p.onSync} aria-label="Sincronizar" title="Sincronizar" data-busy={p.syncing || undefined} disabled={!p.canSync}>
          <ArrowsClockwise size={16} weight="bold" className="sync-icon" />
        </button>
        <button type="button" className="icon-btn add-account" onClick={p.onAddAccount} aria-label="Adicionar conta" title="Adicionar conta">
          <Plus size={16} weight="bold" />
        </button>
        <button type="button" className="icon-btn" onClick={p.onSettings} aria-label="Configurações" title="Configurações">
          <GearSix size={16} weight="bold" />
        </button>
      </div>
    </header>
  );
}
