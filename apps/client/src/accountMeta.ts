// Identidade da conta na interface: apelido (`label:<id>`) e cor (`color:<id>`, ver accountColors.ts).
import type { Account, Settings } from "./api";
import { ACCOUNT_COLORS, colorKey } from "./accountColors";

export const labelKey = (accountId: string) => `label:${accountId}`;

/** Nome padrão (sem apelido): a parte antes do @, o provedor se ela se repete, ou o endereço inteiro. */
export function defaultLabel(accounts: Account[] | null, id: string): string {
  const a = accounts?.find((x) => x.id === id);
  if (!a) return id;
  const others = accounts!.filter((x) => x.id !== id);
  const local = a.username.split("@")[0];
  if (!others.some((x) => x.username.split("@")[0] === local)) return local;
  // Mesma parte local em duas contas (renan@gmail, renan@empresa): o nome do provedor diferencia.
  const host = (u: string) => (u.split("@")[1] ?? "").split(".")[0];
  return host(a.username) && !others.some((x) => host(x.username) === host(a.username)) ? host(a.username) : a.username;
}

/** Apelido escolhido; senão o nome padrão. */
export function accountLabel(accounts: Account[] | null, id: string, settings: Settings): string {
  return settings[labelKey(id)]?.trim() || defaultLabel(accounts, id);
}

/** Cor escolhida; sem ela, uma da paleta pela posição da conta (contas diferentes não repetem cor até seis). */
export function accountColor(accounts: Account[] | null, id: string, settings: Settings): string {
  const set = settings[colorKey(id)];
  if (set) return set;
  if ((accounts?.length ?? 0) < 2) return "green"; // padrão histórico com uma conta só
  const i = Math.max(0, accounts?.findIndex((a) => a.id === id) ?? 0);
  return ACCOUNT_COLORS[i % ACCOUNT_COLORS.length].id;
}
