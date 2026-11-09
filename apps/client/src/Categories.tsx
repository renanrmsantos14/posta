// Categorias coloridas: paleta pastel do tema, leitura das definições e o menu de aplicar.
import { useEffect, useRef, useState } from "react";
import { Check, Tag } from "@phosphor-icons/react";
import type { Category } from "./api";

export const PALETTE = [
  { key: "blue", label: "Azul" },
  { key: "green", label: "Verde" },
  { key: "red", label: "Vermelho" },
  { key: "yellow", label: "Amarelo" },
  { key: "purple", label: "Roxo" },
  { key: "orange", label: "Laranja" },
] as const;

export const MAX_CATEGORIES = 6;

const DEFAULTS: Category[] = [
  { id: "trabalho", name: "Trabalho", color: "blue" },
  { id: "pessoal", name: "Pessoal", color: "green" },
  { id: "importante", name: "Importante", color: "red" },
];

/** `settings.categories` é um JSON; ausente = três categorias de exemplo, "[]" = nenhuma. */
export function parseCategories(raw: string | undefined): Category[] {
  if (raw === undefined || raw === "") return DEFAULTS;
  try {
    const list: unknown = JSON.parse(raw);
    if (!Array.isArray(list)) return DEFAULTS;
    return list
      .filter((c): c is Category => !!c && typeof c.id === "string" && typeof c.name === "string" && typeof c.color === "string")
      .slice(0, MAX_CATEGORIES);
  } catch {
    return DEFAULTS;
  }
}

export const colorVars = (color: string) => ({ background: `var(--${color}-bg)`, color: `var(--${color})` });

export function CategoryChip({ category }: { category: Category }) {
  return <span className="cat-chip" style={colorVars(category.color)}>{category.name}</span>;
}

export function CategoryDot({ color }: { color: string }) {
  return <span className="cat-dot" style={{ background: `var(--${color})` }} aria-hidden />;
}

interface PickerProps {
  categories: Category[];
  /** Categoria atual da seleção; `undefined` quando as mensagens têm categorias diferentes. */
  current: string | null | undefined;
  onPick: (id: string | null) => void;
  /** Mesmo visual dos outros botões da barra. */
  className: string;
}

/** Botão "Categoria" com menu; clicar fora ou Esc fecha. */
export function CategoryPicker({ categories, current, onPick, className }: PickerProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!root.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); setOpen(false); } };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", esc, true);
    return () => { window.removeEventListener("pointerdown", away, true); window.removeEventListener("keydown", esc, true); };
  }, [open]);

  return (
    <span className="cat-picker" ref={root}>
      <button type="button" className={className} aria-label="Categoria" title="Categoria" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <Tag size={15} weight="bold" />
      </button>
      {open && (
        <div className="cat-menu" role="menu" aria-label="Categoria">
          {categories.map((c) => (
            <button key={c.id} type="button" role="menuitemradio" aria-checked={current === c.id} onClick={() => { onPick(c.id); setOpen(false); }}>
              <CategoryDot color={c.color} />
              <span>{c.name}</span>
              {current === c.id && <Check size={13} weight="bold" />}
            </button>
          ))}
          {categories.length === 0 && <p className="cat-empty">Crie categorias em Configurações.</p>}
          <button type="button" role="menuitem" className="cat-none" onClick={() => { onPick(null); setOpen(false); }}>Nenhuma</button>
        </div>
      )}
    </span>
  );
}
