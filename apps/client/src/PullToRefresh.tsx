// Puxar para sincronizar (celular, onde o botão Sincronizar só existe para leitor de tela): no topo da lista, arrastar para baixo
// desce o indicador do título para a lista; soltar depois do limite sincroniza.
import { useRef, useState, type TouchEvent } from "react";
import { ArrowsClockwise } from "@phosphor-icons/react";
import { LOCK_PX } from "./swipe";

/** Distância já amortecida que dispara a sincronização; o dedo anda o dobro. */
const LIMIT = 64;
const MAX = 96;
/** Repouso do indicador acima da borda de baixo do título (igual ao `.ptr` no CSS). */
const REST = -52;

/** `onRefresh` ausente desliga o gesto (desktop ou sem conta); `busy` é a sincronização em andamento, venha de onde vier.
 *  O indicador só gira na sincronização que o puxar disparou: a periódica não cobre a primeira linha. */
export function usePullToRefresh(busy: boolean, onRefresh?: () => void) {
  const ind = useRef<HTMLDivElement>(null);
  const start = useRef<{ x: number; y: number } | null>(null);
  const dist = useRef(0);
  const [pulled, setPulled] = useState(false);
  if (pulled && !busy) setPulled(false);
  const spin = pulled && busy;

  const reset = () => {
    const el = ind.current;
    if (el) { el.removeAttribute("data-drag"); el.style.transform = ""; el.style.opacity = ""; }
    start.current = null;
    dist.current = 0;
  };

  const bind = onRefresh && {
    onTouchStart: (e: TouchEvent) => {
      // Só com tudo entre o dedo e o painel no topo (lista, rascunhos, menus abertos); estado vazio não rola e também vale.
      let scrolled = false;
      for (let n = e.target as Element | null; n && n !== e.currentTarget; n = n.parentElement) if (n.scrollTop > 0) { scrolled = true; break; }
      const t = e.touches[0];
      start.current = busy || e.touches.length > 1 || scrolled ? null : { x: t.clientX, y: t.clientY };
    },
    onTouchMove: (e: TouchEvent) => {
      const s = start.current, el = ind.current;
      if (!s || !el) return;
      const dy = e.touches[0].clientY - s.y, dx = e.touches[0].clientX - s.x;
      if (dist.current === 0) {
        // Espera o dedo andar (como o deslizar da linha) antes de decidir; para cima ou de lado não é puxar.
        if (Math.max(Math.abs(dx), Math.abs(dy)) < LOCK_PX) return;
        if (dy <= 0 || Math.abs(dx) > dy) { start.current = null; return; }
      }
      dist.current = Math.min(MAX, Math.max(0, dy) / 2);
      el.setAttribute("data-drag", "");
      el.style.transform = `translateY(${REST + dist.current}px) rotate(${dist.current * 3}deg)`;
      el.style.opacity = String(Math.min(1, dist.current / LIMIT));
    },
    onTouchEnd: () => {
      const fire = dist.current >= LIMIT;
      reset();
      if (fire) { setPulled(true); onRefresh(); }
    },
    onTouchCancel: reset,
  };

  const indicator = onRefresh && (
    <div ref={ind} className="ptr" data-busy={spin || undefined} role="status">
      <ArrowsClockwise size={18} weight="bold" aria-hidden />
      <span className="sr-only">{spin ? "Sincronizando" : ""}</span>
    </div>
  );
  return { bind, indicator };
}
