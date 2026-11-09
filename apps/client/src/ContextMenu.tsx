// Menu de contexto (botão direito / tecla Menu): portal, teclado completo e submenus de um nível.
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CaretRight } from "@phosphor-icons/react";
import type { Icon } from "@phosphor-icons/react";

export interface MenuItem {
  label?: string;
  icon?: Icon;
  /** Atalho mostrado à direita. */
  kbd?: string;
  onSelect?: () => void;
  disabled?: boolean;
  danger?: boolean;
  /** Linha divisória (sem rótulo). */
  separator?: boolean;
  /** Submenu: o item só abre a lista. */
  items?: MenuItem[];
}

export interface MenuState { x: number; y: number; items: MenuItem[]; label: string }

function Items({ items, onClose, label, sub, onLeave }: { items: MenuItem[]; onClose: () => void; label: string; sub?: boolean; onLeave?: () => void }) {
  const [open, setOpen] = useState<number | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const [flip, setFlip] = useState(false);

  useLayoutEffect(() => {
    if (sub && root.current) setFlip(root.current.getBoundingClientRect().right > window.innerWidth - 4);
  }, [sub]);

  const buttons = () => [...(root.current?.querySelectorAll<HTMLButtonElement>(":scope > .ctx-wrap > button:not(:disabled)") ?? [])];
  useEffect(() => { if (!sub) buttons()[0]?.focus(); }, [sub]);

  const onKey = (e: React.KeyboardEvent) => {
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    const move = (n: number) => { e.preventDefault(); e.stopPropagation(); list[(n + list.length) % list.length]?.focus(); };
    if (e.key === "ArrowDown") move(i + 1);
    else if (e.key === "ArrowUp") move(i < 0 ? list.length - 1 : i - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(list.length - 1);
    else if (e.key === "ArrowLeft" && sub) { e.preventDefault(); e.stopPropagation(); onLeave?.(); }
    else if (e.key === "Tab") { e.preventDefault(); onClose(); }
  };

  return (
    <div ref={root} className="ctx-menu" role="menu" aria-label={label} data-sub={sub || undefined} data-flip={flip || undefined} onKeyDown={onKey}>
      {items.map((it, idx) => it.separator ? <div key={idx} className="ctx-sep" role="separator" /> : (
        <div key={idx} className="ctx-wrap" onMouseEnter={() => it.items && setOpen(idx)} onMouseLeave={() => it.items && setOpen((o) => (o === idx ? null : o))}>
          <button type="button" role="menuitem" className="ctx-item" disabled={it.disabled} data-danger={it.danger || undefined}
            aria-haspopup={it.items ? "menu" : undefined} aria-expanded={it.items ? open === idx : undefined}
            onClick={() => { if (it.items) setOpen(idx); else { onClose(); it.onSelect?.(); } }}
            onKeyDown={(e) => { if (it.items && (e.key === "ArrowRight" || e.key === "Enter" || e.key === " ")) { e.preventDefault(); e.stopPropagation(); setOpen(idx); } }}>
            {it.icon && <it.icon size={15} weight="bold" aria-hidden />}
            <span className="ctx-label">{it.label}</span>
            {it.kbd && <kbd>{it.kbd}</kbd>}
            {it.items && <CaretRight size={12} weight="bold" aria-hidden />}
          </button>
          {it.items && open === idx && (
            <SubMenu items={it.items} label={it.label ?? ""} onClose={onClose} onLeave={() => { const wrap = root.current?.querySelector<HTMLElement>(":scope > .ctx-wrap:has(> .ctx-subhost) > button"); setOpen(null); requestAnimationFrame(() => wrap?.focus()); }} />
          )}
        </div>
      ))}
    </div>
  );
}

function SubMenu({ items, label, onClose, onLeave }: { items: MenuItem[]; label: string; onClose: () => void; onLeave: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  // Entra com o foco no primeiro item; ← devolve o foco ao item pai.
  useEffect(() => { ref.current?.querySelector<HTMLButtonElement>(":scope > .ctx-menu > .ctx-wrap > button:not(:disabled)")?.focus(); }, []);
  return (
    <div ref={ref} className="ctx-subhost">
      <Items items={items} onClose={onClose} label={label} sub onLeave={onLeave} />
    </div>
  );
}

export function ContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: menu.x, y: menu.y });
  const prev = useRef<Element | null>(document.activeElement);

  useLayoutEffect(() => {
    const r = box.current?.getBoundingClientRect();
    if (!r) return;
    setPos({
      x: Math.max(4, Math.min(menu.x, window.innerWidth - r.width - 4)),
      y: Math.max(4, Math.min(menu.y, window.innerHeight - r.height - 4)),
    });
  }, [menu.x, menu.y]);

  const close = useCallback(() => { onClose(); (prev.current as HTMLElement | null)?.focus?.(); }, [onClose]);

  useEffect(() => {
    const away = (e: PointerEvent) => { if (!box.current?.contains(e.target as Node)) onClose(); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); close(); } };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", esc, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", esc, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose, close]);

  return createPortal(
    <div ref={box} className="ctx-root" style={{ left: pos.x, top: pos.y }} onContextMenu={(e) => e.preventDefault()}>
      <Items items={menu.items} onClose={close} label={menu.label} />
    </div>,
    document.body,
  );
}
