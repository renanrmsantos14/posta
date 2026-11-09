// Regras do assistente de conta: servidores conhecidos, descoberta pelo domínio e tradução dos erros do teste.
import { api, type CheckFailure, type CheckStep, type Hosting, type ServerGuess } from "./api";

export interface Setup {
  hosting: Hosting;
  imap: ServerGuess;
  smtp: ServerGuess;
  /** De onde vieram os servidores: lista local, autoconfig do provedor ou palpite `imap.<domínio>`. */
  source: "preset" | "autoconfig" | "guess";
}

const tls = (host: string, port = 993): ServerGuess => ({ host, port, security: "tls" });
const starttls = (host: string, port = 587): ServerGuess => ({ host, port, security: "starttls" });

const GMAIL: Setup = { hosting: "google", imap: tls("imap.gmail.com"), smtp: tls("smtp.gmail.com", 465), source: "preset" };
const OUTLOOK: Setup = { hosting: "microsoft", imap: tls("outlook.office365.com"), smtp: starttls("smtp-mail.outlook.com"), source: "preset" };
const M365_SMTP = starttls("smtp.office365.com");
const YAHOO: Setup = { hosting: "other", imap: tls("imap.mail.yahoo.com"), smtp: tls("smtp.mail.yahoo.com", 465), source: "preset" };

const PRESETS: Record<string, Setup> = {
  "gmail.com": GMAIL, "googlemail.com": GMAIL,
  "outlook.com": OUTLOOK, "hotmail.com": OUTLOOK, "live.com": OUTLOOK, "msn.com": OUTLOOK,
  "outlook.com.br": OUTLOOK, "hotmail.com.br": OUTLOOK, "live.com.br": OUTLOOK,
  "icloud.com": { hosting: "other", imap: tls("imap.mail.me.com"), smtp: starttls("smtp.mail.me.com"), source: "preset" },
  "yahoo.com": YAHOO, "yahoo.com.br": YAHOO,
};

export const domainOf = (email: string) => email.split("@")[1]?.trim().toLowerCase() ?? "";

/** Servidores e hospedagem do email: lista local primeiro, depois autoconfig + MX (só o domínio sai da máquina). */
export async function detectSetup(email: string): Promise<Setup> {
  const domain = domainOf(email);
  const known = PRESETS[domain];
  if (known) return known;
  // O backend tem prazo próprio (~4 s); o teto aqui cobre um backend que nem responde.
  const det = await Promise.race([
    api.detectProvider(domain),
    new Promise<null>((r) => setTimeout(() => r(null), 6000)),
  ]).catch(() => null);
  const hosting = det?.hosting ?? "other";
  if (hosting === "google") return { ...GMAIL, source: "autoconfig" };
  if (hosting === "microsoft") return { hosting, imap: det?.imap ?? OUTLOOK.imap, smtp: det?.smtp ?? M365_SMTP, source: det?.imap ? "autoconfig" : "preset" };
  return {
    hosting,
    imap: det?.imap ?? tls(`imap.${domain}`),
    smtp: det?.smtp ?? tls(`smtp.${domain}`, 465),
    source: det?.imap ? "autoconfig" : "guess",
  };
}

/** Falhas que se resolvem mexendo nos servidores, não na senha. */
export const isServerFailure = (f: CheckFailure) => ["dns", "timeout", "refused", "tls"].includes(f.kind) || (f.step === "smtp" && f.kind === "auth");

export function asFailure(e: unknown): CheckFailure {
  const o = e as Partial<CheckFailure> | null;
  if (o && typeof o === "object" && typeof o.kind === "string") return { step: o.step ?? "login", kind: o.kind, detail: o.detail ?? "" };
  return { step: "login", kind: "unknown", detail: String(e) };
}

export const STEP_LABEL: Record<CheckStep, string> = {
  connect: "Conectando ao servidor",
  login: "Conferindo email e senha",
  smtp: "Conferindo o envio",
};

/** Mensagem para a pessoa: o que houve e o que fazer. O erro cru fica em "Detalhes". */
export function explain(f: CheckFailure, ctx: { hosting: Hosting; host: string; port: number; canMicrosoft: boolean }): { title: string; hint: string } {
  const { hosting, host, port } = ctx;
  switch (f.kind) {
    case "dns":
      return { title: `Não encontrei o servidor ${host}.`, hint: "Confira o endereço em Configuração manual. Se estiver certo, veja se o computador está com internet." };
    case "timeout":
      return { title: `O servidor ${host} não respondeu a tempo.`, hint: "Pode ser a porta errada, um bloqueio de rede ou conexão lenta. Tente de novo ou confira a porta em Configuração manual." };
    case "refused":
      return { title: `O servidor ${host} recusou a conexão na porta ${port}.`, hint: "Essa porta provavelmente está errada. No IMAP o usual é 993 (TLS) ou 143 (STARTTLS); no envio, 465 ou 587." };
    case "tls":
      return { title: `Não consegui abrir uma conexão segura com ${host}.`, hint: "O certificado do servidor não é confiável ou a segurança não combina com a porta (993 usa TLS; 143 e 587 usam STARTTLS)." };
    case "offline":
      return { title: "O computador está sem conexão com a internet.", hint: "Conecte-se e tente de novo." };
    case "imap_disabled":
      return {
        title: "O acesso IMAP está desativado nesta conta.",
        hint: hosting === "google" ? "No Gmail, abra Configurações › Encaminhamento e POP/IMAP e ative o IMAP."
          : hosting === "microsoft" ? "O administrador do Microsoft 365 desativou o acesso por IMAP. Use Entrar com Microsoft ou peça a ele para liberar."
          : "Ative o acesso por IMAP nas configurações da conta, no site do provedor.",
      };
    case "auth":
      if (f.step === "smtp") return { title: "O servidor de envio recusou o login.", hint: "A leitura funcionou, mas o envio não. Confira o servidor de envio (SMTP) em Configuração manual." };
      if (hosting === "google") return { title: "O Google recusou a senha.", hint: "No Gmail não vale a senha normal: use uma senha de app de 16 letras (exige verificação em duas etapas)." };
      if (hosting === "microsoft") return { title: "A Microsoft recusou a senha.", hint: ctx.canMicrosoft ? "Contas Microsoft costumam exigir login pelo navegador. Use Entrar com Microsoft." : "Contas Microsoft costumam exigir uma senha de app (com verificação em duas etapas) ou o login pelo navegador." };
      return { title: "Email ou senha recusados pelo servidor.", hint: "Confira o email completo e a senha. Se a conta usa verificação em duas etapas, gere uma senha de app." };
    default:
      return { title: "Não consegui conectar.", hint: "Veja os detalhes abaixo ou confira a Configuração manual." };
  }
}
