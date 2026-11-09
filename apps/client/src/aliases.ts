// Remetentes alternativos (aliases) de uma conta: configuração `aliases:<id da conta>`, uma linha por alias
// (`end@x.com` ou `Nome <end@x.com>`).

export const aliasLines = (text: string | undefined): string[] =>
  (text ?? "").split("\n").map((s) => s.trim()).filter(Boolean);

/** Endereço de `Nome <end@x.com>` ou `end@x.com`, em minúsculas. */
export const addrOf = (s: string): string => (/<([^>]+)>/.exec(s)?.[1] ?? s).trim().toLowerCase();

/** O alias para o qual a mensagem foi enviada (em Para ou Cc), se houver. */
export function aliasFor(aliases: string[], toAddrs: string, ccAddrs: string): string | undefined {
  const got = new Set(`${toAddrs},${ccAddrs}`.split(/[,;]/).map(addrOf).filter(Boolean));
  return aliases.find((a) => got.has(addrOf(a)));
}
