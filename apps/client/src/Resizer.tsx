// Borda arrastável entre painéis: pointer capture, limites, clique duplo restaura o padrão, setas ajustam.
import { useRef, useState } from "react";

interface Props {
  axis: "x" | "y";
  /** Identifica a borda no CSS (posição) e nos testes. */
  edge: "rail" | "list";
  label: string;
  value: number;
  min: number;
  max: () => number;
  /** Durante o arrasto (só visual). */
  onChange: (px: number) => void;
  /** Ao soltar ou usar o teclado (persiste). */
  onCommit: (px: number) => void;
  onReset: () => void;
}

const STEP = 16;

export function Resizer({ axis, edge, label, value, min, max, onChange, onCommit, onReset }: Props) {
  const drag = useRef<{ start: number; from: number; last: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const clamp = (n: number) => Math.round(Math.min(Math.max(n, min), Math.max(min, max())));
  const pos = (e: React.PointerEvent) => (axis === "x" ? e.clientX : e.clientY);

  function finish() {
    const d = drag.current;
    drag.current = null;
    setDragging(false);
    if (d && d.last !== d.from) onCommit(d.last);
  }

  return (
    <div
      role="separator" tabIndex={0} className="resizer" data-axis={axis} data-edge={edge} data-dragging={dragging || undefined}
      aria-label={label} aria-orientation={axis === "x" ? "vertical" : "horizontal"}
      aria-valuenow={value} aria-valuemin={min}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { start: pos(e), from: value, last: value };
        setDragging(true);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        d.last = clamp(d.from + pos(e) - d.start);
        onChange(d.last);
      }}
      onPointerUp={finish}
      onPointerCancel={finish}
      onDoubleClick={onReset}
      onKeyDown={(e) => {
        const dir = axis === "x" ? { ArrowLeft: -1, ArrowRight: 1 }[e.key] : { ArrowUp: -1, ArrowDown: 1 }[e.key];
        if (!dir) return;
        // Setas aqui ajustam a borda; não devem navegar pela lista (atalho global).
        e.preventDefault();
        e.stopPropagation();
        onCommit(clamp(value + dir * STEP));
      }}
    />
  );
}
