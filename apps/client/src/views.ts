// Visões da lista: pastas por papel, pasta do usuário (`box:`) e categoria (`cat:`). Sem dependências, para api e mock usarem.
import type { Role, View } from "./api";

export const boxView = (account: string, name: string): View => `box:${encodeURIComponent(account)}|${encodeURIComponent(name)}`;
export const catView = (id: string): View => `cat:${encodeURIComponent(id)}`;

export function parseBox(view: View): { account: string; name: string } | null {
  if (!view.startsWith("box:")) return null;
  const [a, n] = view.slice(4).split("|");
  return { account: decodeURIComponent(a), name: decodeURIComponent(n) };
}

export const parseCat = (view: View): string | null => (view.startsWith("cat:") ? decodeURIComponent(view.slice(4)) : null);

/** Papel de uma pasta de sistema, ou null para pasta do usuário, categoria e "Com estrela". */
export const roleOf = (view: View): Role | null => (view.startsWith("box:") || view.startsWith("cat:") || view === "starred" ? null : (view as Role));
