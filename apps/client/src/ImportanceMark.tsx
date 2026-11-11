import { ArrowDown, WarningCircle } from "@phosphor-icons/react";

/** Marcador discreto de importância: alta (!) ou baixa (seta); normal não mostra nada. */
export function ImportanceMark({ value }: { value?: number }) {
  if (value === 1) return <WarningCircle size={12} weight="fill" className="imp-mark" data-imp="high" aria-label="Importância alta" role="img" />;
  if (value === -1) return <ArrowDown size={12} weight="bold" className="imp-mark" data-imp="low" aria-label="Importância baixa" role="img" />;
  return null;
}
