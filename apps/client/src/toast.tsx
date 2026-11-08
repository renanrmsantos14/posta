// Toasts mínimos: chame toast() de qualquer lugar e monte <Toaster /> uma vez.
import { useEffect, useState, useSyncExternalStore } from "react";
import { CheckCircle, WarningCircle, Info } from "@phosphor-icons/react";

type Kind = "success" | "error" | "info";
interface Item { id: number; text: string; kind: Kind; leaving: boolean }

let items: Item[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

const DURATION = { success: 3000, info: 3000, error: 6000 } as const;
const EXIT_MS = 160;

function dismiss(id: number) {
  items = items.map((t) => (t.id === id ? { ...t, leaving: true } : t));
  emit();
  setTimeout(() => {
    items = items.filter((t) => t.id !== id);
    emit();
  }, EXIT_MS);
}

export function toast(text: string, kind: Kind = "info") {
  items = [...items.slice(-2), { id: nextId++, text, kind, leaving: false }];
  emit();
}

const icons = { success: CheckCircle, error: WarningCircle, info: Info };

function ToastItem({ item }: { item: Item }) {
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (paused || item.leaving) return;
    // Não some enquanto a janela está em segundo plano: ninguém teria lido.
    let timer = 0;
    const arm = () => {
      clearTimeout(timer);
      if (!document.hidden) timer = window.setTimeout(() => dismiss(item.id), DURATION[item.kind]);
    };
    arm();
    document.addEventListener("visibilitychange", arm);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", arm);
    };
  }, [paused, item]);
  const Icon = icons[item.kind];
  return (
    <div
      className="toast"
      data-kind={item.kind}
      data-leaving={item.leaving || undefined}
      role={item.kind === "error" ? "alert" : "status"}
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onClick={() => dismiss(item.id)}
    >
      <Icon size={16} weight="fill" aria-hidden />
      <span>{item.text}</span>
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
