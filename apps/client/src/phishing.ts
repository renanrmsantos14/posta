// Sinais claros de remetente suspeito. Puro (sem rede): compara só os cabeçalhos já lidos.

const EMAIL = /[a-z0-9._%+-]+@([a-z0-9.-]+\.[a-z]{2,})/i;
/** Segundo nível comum sob TLD de país (com.br, co.uk): o domínio registrável tem três rótulos. */
const SLD = new Set(["com", "net", "org", "gov", "edu", "co", "ac"]);

const domainOf = (addr: string): string => {
  const m = EMAIL.exec(/<([^>]+)>/.exec(addr)?.[1] ?? addr);
  return m ? m[1].toLowerCase() : "";
};

/** Domínio registrável aproximado: `mail.banco.com.br` → `banco.com.br`. */
export const baseDomain = (host: string): string => {
  const p = host.toLowerCase().split(".").filter(Boolean);
  const n = p.length >= 3 && p[p.length - 1].length === 2 && SLD.has(p[p.length - 2]) ? 3 : 2;
  return p.slice(-n).join(".");
};

/** Motivo (uma frase) de a mensagem parecer falsa, ou null quando nada chama atenção. */
export function suspiciousSender(fromName: string, fromAddr: string, replyAddr: string): string | null {
  const from = domainOf(fromAddr);
  if (!from) return null;
  if (from.split(".").some((l) => l.startsWith("xn--"))) return `O domínio do remetente (${from}) usa caracteres que imitam outro endereço.`;
  const shown = domainOf(fromName);
  if (shown && baseDomain(shown) !== baseDomain(from)) return `O nome exibido contém o endereço ${EMAIL.exec(fromName)![0]}, mas a mensagem veio de ${from}.`;
  const reply = domainOf(replyAddr);
  if (reply && baseDomain(reply) !== baseDomain(from)) return `As respostas vão para ${reply}, um domínio diferente do remetente (${from}).`;
  return null;
}
