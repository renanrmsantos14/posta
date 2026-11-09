// Fantasma do arrasto de mensagens: segue o ponteiro (posição por transform, sem renderizar) e mostra a
// contagem; "+" quando soltar copia (Ctrl). Não captura o ponteiro, para o alvo sob ele receber o hit-test.
import { useLayoutEffect, type MutableRefObject } from "react";
import { Plus } from "@phosphor-icons/react";

export function placeGhost(el: HTMLElement | null, x: number, y: number) {
  if (el) el.style.transform = `translate3d(${x + 14}px, ${y + 14}px, 0)`;
}

interface Props {
  elRef: MutableRefObject<HTMLDivElement | null>;
  pos: MutableRefObject<{ x: number; y: number }>;
  count: number;
  copy: boolean;
  label: string;
}

export function DragGhost({ elRef, pos, count, copy, label }: Props) {
  useLayoutEffect(() => placeGhost(elRef.current, pos.current.x, pos.current.y), [elRef, pos]);
  return (
    <div ref={elRef} className="drag-ghost" aria-hidden data-copy={copy || undefined} data-many={count > 1 || undefined}>
      <span className="drag-ghost-n">{count}</span>
      {copy && <Plus size={12} weight="bold" />}
      <span className="drag-ghost-label">{count > 1 ? "mensagens" : label}</span>
    </div>
  );
}
