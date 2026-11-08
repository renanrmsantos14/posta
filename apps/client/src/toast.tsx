// Toasts mínimos: chame toast() de qualquer lugar e monte <Toaster /> uma vez.
import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { CheckCircle, WarningCircle, Info, PaperPlaneTilt } from "@phosphor-icons/react";

type Kind = "success" | "error" | "info" | "sending";
/** Por que o toast fechou: o usuário clicou na ação, o tempo acabou ou clicou no toast. */
export type CloseReason = "action" | "timeout" | "click";

interface Options {
  action?: { label: string; onClick: () => void };
  /** Em ms. Com ação, mostra a barra de tempo restante. */
  duration?: number;
  onClose?: (reason: CloseReason) => void;
}
interface Item extends Options { id: number; text: string; kind: Kind; leaving: boolean }

let items: Item[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

const DURATION: Record<Kind, number> = { success: 3000, info: 3000, error: 6000, sending: 4000 };
const EXIT_MS = 160;

function dismiss(id: number, reason: CloseReason) {
  const item = items.find((t) => t.id === id);
  if (!item || item.leaving) return;
  items = items.map((t) => (t.id === id ? { ...t, leaving: true } : t));
  emit();
  item.onClose?.(reason);
  setTimeout(() => {
    items = items.filter((t) => t.id !== id);
    emit();
  }, EXIT_MS);
}

export function toast(text: string, kind: Kind = "info", options: Options = {}) {
  // Mantém no máximo 3; quem sai por excesso conta como "tempo acabou".
  items.slice(0, Math.max(0, items.length - 2)).forEach((t) => !t.leaving && t.onClose?.("timeout"));
  items = [...items.slice(-2), { id: nextId++, text, kind, leaving: false, ...options }];
  emit();
}

const icons = { success: CheckCircle, error: WarningCircle, info: Info, sending: PaperPlaneTilt };

function ToastItem({ item }: { item: Item }) {
  const [paused, setPaused] = useState(false);
  const total = item.duration ?? DURATION[item.kind];
  // Tempo restante sobrevive a pausas (hover, janela em segundo plano).
  const remaining = useRef(total);

  useEffect(() => {
    if (paused || item.leaving) return;
    let timer = 0;
    let startedAt = 0;
    const stop = () => {
      if (!timer) return;
      clearTimeout(timer);
      timer = 0;
      remaining.current -= performance.now() - startedAt;
    };
    const arm = () => {
      stop();
      if (document.hidden) return; // ninguém está vendo: não conta o tempo
      startedAt = performance.now();
      timer = window.setTimeout(() => dismiss(item.id, "timeout"), Math.max(0, remaining.current));
    };
    arm();
    document.addEventListener("visibilitychange", arm);
    return () => {
      stop();
      document.removeEventListener("visibilitychange", arm);
    };
  }, [paused, item]);

  const Icon = icons[item.kind];
  return (
    <div
      className="toast"
      data-kind={item.kind}
      data-leaving={item.leaving || undefined}
      data-paused={paused || undefined}
      role={item.kind === "error" ? "alert" : "status"}
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onClick={() => dismiss(item.id, "click")}
    >
      <Icon size={16} weight="fill" aria-hidden />
      <span>{item.text}</span>
      {item.action && (
        <>
          <button
            className="toast-action"
            onClick={(e) => {
              e.stopPropagation();
              item.action!.onClick();
              dismiss(item.id, "action");
            }}
          >
            {item.action.label}
          </button>
          <span className="toast-timer" style={{ animationDuration: `${total}ms` }} aria-hidden />
        </>
      )}
    </div>
  );
}

export function Toaster() {
  const list = useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => items,
  );
  return (
    <div className="toaster" aria-live="polite">
      {list.map((t) => <ToastItem key={t.id} item={t} />)}
    </div>
  );
}
