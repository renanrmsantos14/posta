// Menu de horário: "Hoje mais tarde", "Amanhã", "Próxima semana" e "Escolher…". Serve para adiar mensagens e agendar envios.
import { useEffect, useRef, useState } from "react";
import { CalendarBlank } from "@phosphor-icons/react";
import { fromLocalInput, toLocalInput, whenLabel, whenPresets } from "./productivity";

/** Fecha o menu ao clicar fora ou com Esc (o Esc não chega aos atalhos da tela). */
export function useDismiss(open: boolean, close: () => void, root: React.RefObject<HTMLElement | null>) {
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) close(); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); close(); } };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", esc, true);
    return () => { window.removeEventListener("pointerdown", away, true); window.removeEventListener("keydown", esc, true); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
}

interface Props {
  /** Conteúdo do botão (ícone e/ou texto). */
  children: React.ReactNode;
  label: string;
  className: string;
  onPick: (unix: number) => void;
  /** Abre para cima (botões no rodapé do Composer). */
  up?: boolean;
  disabled?: boolean;
  /** Texto do primeiro item ("Hoje mais tarde"). */
  later?: string;
}

export function WhenMenu({ children, label, className, onPick, up, disabled, later }: Props) {
  const [open, setOpen] = useState(false);
  const [custom, setCustom] = useState<string | null>(null);
  const root = useRef<HTMLSpanElement>(null);

  useDismiss(open, () => setOpen(false), root);

  const now = Math.floor(Date.now() / 1000);
  const customAt = custom ? fromLocalInput(custom) : NaN;
  const pick = (at: number) => { setOpen(false); setCustom(null); onPick(at); };

  return (
    <span className="cat-picker" ref={root}>
      <button type="button" className={className} aria-label={label} title={label} aria-haspopup="menu" aria-expanded={open}
        disabled={disabled} onClick={() => { setOpen((o) => !o); setCustom(null); }}>
        {children}
      </button>
      {open && (
        <div className="cat-menu when-menu" data-up={up || undefined} role="menu" aria-label={label}>
          {whenPresets(now, later).map((p) => (
            <button key={p.key} type="button" role="menuitem" onClick={() => pick(p.at)}>
              <span>{p.label}</span><small>{whenLabel(p.at)}</small>
            </button>
          ))}
          {custom === null ? (
            <button type="button" role="menuitem" className="cat-none" onClick={() => setCustom(toLocalInput(now + 3600))}>
              <CalendarBlank size={14} weight="bold" /> <span>Escolher data e hora…</span>
            </button>
          ) : (
            <form className="when-custom" onSubmit={(e) => { e.preventDefault(); if (customAt > now) pick(customAt); }}>
              <input type="datetime-local" autoFocus aria-label="Data e hora" value={custom} min={toLocalInput(now)} onChange={(e) => setCustom(e.target.value)} />
              <button type="submit" className="btn small primary" disabled={!(customAt > now)}>Confirmar</button>
            </form>
          )}
        </div>
      )}
    </span>
  );
}
