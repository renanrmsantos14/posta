// Botão Escrever do celular: some ao descer a lista (libera a última linha) e volta ao subir.
import { useCallback, useRef, type RefObject } from "react";
import { NotePencil } from "@phosphor-icons/react";

/** Rolagem mínima, em px, para trocar de estado; evita piscar com tremidas do dedo. */
const STEP = 8;

/** Liga o Escrever à rolagem da lista. Direto no DOM: rolar não deve renderizar o App a cada quadro. */
export function useHideOnScroll() {
  const ref = useRef<HTMLButtonElement>(null);
  const last = useRef(0);
  const onScroll = useCallback((y: number) => {
    if (Math.abs(y - last.current) < STEP) return;
    ref.current?.toggleAttribute("data-hidden", y > last.current);
    last.current = y;
  }, []);
  return { ref, onScroll };
}

export function Fab({ fabRef, onClick }: { fabRef: RefObject<HTMLButtonElement | null>; onClick: () => void }) {
  return (
    <button ref={fabRef} className="fab mobile-only" onClick={onClick} aria-label="Escrever">
      <NotePencil size={22} weight="bold" />
    </button>
  );
}
