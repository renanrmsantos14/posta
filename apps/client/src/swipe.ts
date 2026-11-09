// Deslizar a linha da lista (só toque): direita e esquerda disparam as ações configuradas.
// O estado fica no módulo (há um só dedo por vez) e a linha é movida direto no DOM, sem re-render a cada pixel.
import type { PointerEvent } from "react";

export type SwipeAction = "archive" | "trash" | "read" | "none";
export type SwipeDir = "left" | "right";

const THRESHOLD = 88;
const LOCK_PX = 10;

interface Gesture { el: HTMLElement; id: number; x: number; y: number; dx: number; lock: "h" | "v" | null }
let g: Gesture | null = null;
let endedAt = 0;

/** O ponteiro principal é toque (celular/tablet); com mouse não há gesto. */
const coarse = () => window.matchMedia("(pointer: coarse)").matches;

/** Um clique logo depois de deslizar não deve abrir a mensagem. */
export const justSwiped = () => Date.now() - endedAt < 350;

function reset(el: HTMLElement) {
  el.style.removeProperty("--swipe-x");
  el.style.removeProperty("--swipe-p");
  delete el.dataset.swipe;
  delete el.dataset.swipeAction;
}

/** Props de ponteiro para a `<li>`; `action` diz o que cada lado faz, `onSwipe` executa. */
export function swipeProps(action: (dir: SwipeDir) => SwipeAction, onSwipe: (dir: SwipeDir) => void) {
  return {
    onPointerDown(e: PointerEvent<HTMLElement>) {
      if (e.pointerType !== "touch" || !coarse() || (e.target as Element).closest("button")) return;
      g = { el: e.currentTarget, id: e.pointerId, x: e.clientX, y: e.clientY, dx: 0, lock: null };
    },
    onPointerMove(e: PointerEvent<HTMLElement>) {
      if (!g || g.id !== e.pointerId) return;
      const dx = e.clientX - g.x;
      const dy = e.clientY - g.y;
      if (!g.lock && Math.max(Math.abs(dx), Math.abs(dy)) > LOCK_PX) {
        g.lock = Math.abs(dx) > Math.abs(dy) * 1.5 ? "h" : "v";
        if (g.lock === "h") g.el.setPointerCapture(e.pointerId);
      }
      if (g.lock !== "h") return;
      g.dx = dx;
      const dir: SwipeDir = dx > 0 ? "right" : "left";
      const what = action(dir);
      if (what === "none") return reset(g.el);
      g.el.dataset.swipe = dir;
      g.el.dataset.swipeAction = what;
      g.el.style.setProperty("--swipe-x", `${dx}px`);
      g.el.style.setProperty("--swipe-p", String(Math.min(1, Math.abs(dx) / THRESHOLD)));
    },
    onPointerUp(e: PointerEvent<HTMLElement>) {
      if (!g || g.id !== e.pointerId) return;
      const { el, dx, lock } = g;
      g = null;
      if (lock !== "h") return;
      endedAt = Date.now();
      reset(el);
      if (Math.abs(dx) >= THRESHOLD) onSwipe(dx > 0 ? "right" : "left");
    },
    onPointerCancel(e: PointerEvent<HTMLElement>) {
      if (!g || g.id !== e.pointerId) return;
      reset(g.el);
      g = null;
    },
  };
}
