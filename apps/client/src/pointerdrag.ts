// Arrasto com o mouse por eventos de ponteiro (em vez do arrastar e soltar do HTML5): dá para
// mostrar o fantasma com a contagem, testar com mouse real e iniciar o arrasto nativo do sistema
// quando o ponteiro sai da janela (o HTML5 e o nativo não convivem).
const THRESHOLD = 5;

export interface PointerDragHandlers {
  /** Passou do limite de 5px: o arrasto começou. */
  start?(x: number, y: number): void;
  move?(x: number, y: number, copy: boolean): void;
  /** Soltou dentro da janela. */
  end?(x: number, y: number, copy: boolean): void;
  /** Esc, perda de foco ou botão solto fora de vista. */
  cancel?(): void;
  /** O ponteiro saiu da janela com o botão pressionado. `released()` diz se o botão já foi solto. */
  leave?(released: () => boolean): void | Promise<void>;
}

/** Começa a acompanhar um arrasto a partir de um `pointerdown`. Só botão esquerdo do mouse. */
export function trackPointerDrag(down: { clientX: number; clientY: number; button: number; pointerType: string }, h: PointerDragHandlers): void {
  if (down.pointerType !== "mouse" || down.button !== 0) return;
  let started = false;
  let left = false;
  let released = false;
  const root = document.documentElement;

  const stop = () => {
    window.removeEventListener("pointermove", onMove, true);
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("blur", onBlur);
    root.removeEventListener("mouseleave", onLeave);
    delete root.dataset.pdrag;
  };
  const finish = () => {
    window.removeEventListener("pointerup", onUp, true);
    stop();
  };
  const swallowClick = () => {
    const eat = (e: Event) => { e.stopPropagation(); e.preventDefault(); };
    window.addEventListener("click", eat, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", eat, true), 0);
  };
  const leave = () => {
    if (left || !started || !h.leave) return;
    left = true;
    stop();
    swallowClick();
    Promise.resolve(h.leave(() => released)).catch(() => {}).finally(() => window.removeEventListener("pointerup", onUp, true));
  };

  function onMove(e: PointerEvent) {
    if (!started) {
      if (Math.hypot(e.clientX - down.clientX, e.clientY - down.clientY) < THRESHOLD) return;
      started = true;
      root.dataset.pdrag = "";
      window.getSelection()?.removeAllRanges();
      h.start?.(e.clientX, e.clientY);
    }
    if (e.buttons === 0) { finish(); h.cancel?.(); return; }
    if (e.clientX < 0 || e.clientY < 0 || e.clientX >= window.innerWidth || e.clientY >= window.innerHeight) { leave(); return; }
    h.move?.(e.clientX, e.clientY, e.ctrlKey);
  }
  function onUp(e: PointerEvent) {
    released = true;
    if (left) return;
    finish();
    if (!started) return;
    swallowClick();
    h.end?.(e.clientX, e.clientY, e.ctrlKey);
  }
  function onKey(e: KeyboardEvent) {
    if (e.key !== "Escape" || !started) return;
    e.preventDefault();
    e.stopPropagation();
    finish();
    swallowClick();
    h.cancel?.();
  }
  function onBlur() {
    finish();
    if (started) h.cancel?.();
  }
  function onLeave() { leave(); }

  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("blur", onBlur);
  root.addEventListener("mouseleave", onLeave);
}

/** Alvo de soltar sob o ponteiro (`data-drop-view`), se houver. */
export const dropViewAt = (x: number, y: number): string | null =>
  document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop-view]")?.dataset.dropView ?? null;
