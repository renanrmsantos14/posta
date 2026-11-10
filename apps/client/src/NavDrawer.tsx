// Gaveta lateral do celular: guarda a Rail (pastas e contas). Fecha no X, ao tocar fora, com Esc, com o Voltar do Android e ao escolher uma pasta.
import { useCallback, useEffect, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";

/** Tempo da saída; mantém em sincronia com `.drawer[data-leaving]` no styles.css. */
const LEAVE_MS = 180;

/**
 * Estado da gaveta. Abrir empilha uma entrada no histórico: o Voltar do Android (que no WebView navega no histórico)
 * fecha a gaveta em vez de sair do app. Fechar por outro caminho desfaz a entrada.
 */
export function useDrawer() {
  const [open, setOpen] = useState(false);
  const pushed = useRef(false);
  const show = useCallback(() => {
    if (pushed.current) return;
    history.pushState({ posta: "drawer" }, "");
    pushed.current = true;
    setOpen(true);
  }, []);
  const hide = useCallback(() => {
    if (pushed.current) {
      pushed.current = false;
      history.back();
    }
    setOpen(false);
  }, []);
  useEffect(() => {
    const onPop = () => {
      if (!pushed.current) return;
      pushed.current = false;
      setOpen(false);
    };
    window.addEventListener("popstate", onPop);
    // Janela alargada (desktop/rotação): a Rail fixa volta e a gaveta não faz mais sentido.
    const wide = window.matchMedia("(min-width: 721px)");
    const onWide = () => wide.matches && hide();
    wide.addEventListener("change", onWide);
    return () => {
      window.removeEventListener("popstate", onPop);
      wide.removeEventListener("change", onWide);
    };
  }, [hide]);
  return { open, show, hide };
}

interface Props {
  open: boolean;
  onClose: () => void;
  /** Botão que abriu a gaveta: recebe o foco de volta ao fechar. */
  returnFocus: React.RefObject<HTMLElement | null>;
  /** Função, não elemento: o conteúdo só é montado com a gaveta aberta ou saindo. */
  children: () => React.ReactNode;
}

export function NavDrawer({ open, onClose, returnFocus, children }: Props) {
  const [mounted, setMounted] = useState(open);
  const [leaving, setLeaving] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const was = useRef(open);

  useEffect(() => {
    if (open) {
      setMounted(true);
      setLeaving(false);
    } else if (was.current) {
      setLeaving(true);
      returnFocus.current?.focus();
      const t = setTimeout(() => { setMounted(false); setLeaving(false); }, LEAVE_MS);
      was.current = open;
      return () => clearTimeout(t);
    }
    was.current = open;
  }, [open, returnFocus]);

  // O painel só existe depois de `mounted`: o foco entra nele nesse momento.
  useEffect(() => { if (open && mounted) panel.current?.focus(); }, [open, mounted]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, onClose]);

  /** Tab fica dentro da gaveta. */
  const trap = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab" || !panel.current) return;
    const items = [...panel.current.querySelectorAll<HTMLElement>("button:not([disabled]), [href], input, [tabindex]:not([tabindex='-1'])")].filter((el) => getComputedStyle(el).visibility !== "hidden");
    if (!items.length) return;
    const first = items[0], last = items[items.length - 1];
    const at = document.activeElement;
    if (e.shiftKey && (at === first || at === panel.current)) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && at === last) { e.preventDefault(); first.focus(); }
  };

  if (!mounted) return null;
  return (
    <div className="drawer" data-leaving={leaving || undefined} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={panel} className="drawer-panel" role="dialog" aria-modal="true" aria-label="Pastas e contas" tabIndex={-1} onKeyDown={trap}>
        <button type="button" className="icon-btn drawer-close" aria-label="Fechar" title="Fechar" onClick={onClose}><X size={16} weight="bold" /></button>
        {children()}
      </div>
    </div>
  );
}
