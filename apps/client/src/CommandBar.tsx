// Faixa de comandos contextual (sempre numa linha). O CSS esconde rótulos e depois itens por largura do próprio painel
// (container queries, pelo `data-pri`); o botão "…" lista na hora de abrir o que ficou escondido.
import { useRef, useState } from "react";
import type { Icon } from "@phosphor-icons/react";
import { DotsThree } from "@phosphor-icons/react";
import { ContextMenu, type MenuItem, type MenuState } from "./ContextMenu";

export interface Cmd {
  id: string;
  label: string;
  icon: Icon;
  /** 1 = último a sumir; números maiores somem primeiro quando a faixa fica estreita. */
  pri: number;
  /** Grupos vizinhos são separados por uma linha fina. */
  group: number;
  kbd?: string;
  primary?: boolean;
  pressed?: boolean;
  disabled?: boolean;
  /** Ação direta. */
  run?: () => void;
  /** Ou um menu (Mover, Adiar, Categorias): abre sob o botão e vira submenu dentro do "…". */
  menu?: () => MenuItem[];
}

type ToolbarSize = "s" | "m" | "l";
type ToolbarLabels = "auto" | "always" | "icons";
const ICON = { s: 16, m: 18, l: 20 } as const;

export function CommandBar({ cmds: all, size = "m", labels = "auto", hidden }: { cmds: Cmd[]; size?: ToolbarSize; labels?: ToolbarLabels; hidden?: Set<string> }) {
  const cmds = hidden?.size ? all.filter((c) => !hidden.has(c.id)) : all;
  const bar = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);

  const open = (items: MenuItem[], label: string, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 4, items, label });
  };

  /** Escondidos pela largura (display: none) ou cortados pela borda da faixa (rótulos "Sempre" com rolagem). */
  const hiddenIds = () => {
    const out = new Set<string>();
    const host = bar.current;
    if (!host) return out;
    const edge = host.getBoundingClientRect();
    const moreEl = host.querySelector<HTMLElement>(".cmd-more");
    const right = moreEl?.getClientRects().length ? moreEl.getBoundingClientRect().left : edge.right;
    host.querySelectorAll<HTMLElement>("[data-cmd]").forEach((el) => {
      const r = el.getBoundingClientRect();
      if (el.getClientRects().length === 0 || r.right > right + 1 || r.left < edge.left - 1) out.add(el.dataset.cmd!);
    });
    return out;
  };

  const asItem = (c: Cmd): MenuItem => c.menu
    ? { label: c.label, icon: c.icon, disabled: c.disabled, items: c.menu() }
    : { label: c.label, icon: c.icon, kbd: c.kbd, disabled: c.disabled, onSelect: c.run };

  const more = (el: HTMLElement) => {
    const gone = hiddenIds();
    const items = cmds.filter((c) => gone.has(c.id)).map(asItem);
    if (items.length) open(items, "Mais ações", el);
  };

  return (
    <div className="cmdbar-host" role="region" aria-label="Barra de comandos" data-size={size} data-labels={labels}>
      <div ref={bar} className="cmdbar" role="toolbar" aria-label="Comandos">
        {cmds.map((c, i) => (
          <span key={c.id} className="cmd-slot" data-cmd={c.id} data-pri={c.pri}>
            {i > 0 && cmds[i - 1].group !== c.group && <span className="cmd-sep" aria-hidden />}
            <button type="button" className="cmd" data-primary={c.primary || undefined} aria-label={c.label} title={c.kbd ? `${c.label} (${c.kbd})` : c.label}
              aria-pressed={c.pressed} aria-haspopup={c.menu ? "menu" : undefined} disabled={c.disabled}
              onClick={(e) => (c.menu ? open(c.menu(), c.label, e.currentTarget) : c.run?.())}>
              <c.icon size={ICON[size]} weight={c.pressed ? "fill" : "bold"} aria-hidden />
              <span className="cmd-label">{c.label}</span>
            </button>
          </span>
        ))}
        <button type="button" className="cmd cmd-more" aria-label="Mais ações" title="Mais ações" aria-haspopup="menu" onClick={(e) => more(e.currentTarget)}>
          <DotsThree size={ICON[size] + 2} weight="bold" aria-hidden />
        </button>
      </div>
      {menu && <ContextMenu menu={menu} onClose={() => setMenu(null)} />}
    </div>
  );
}
