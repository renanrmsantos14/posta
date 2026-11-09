// Dados de exemplo para rodar a UI no navegador (pnpm dev), sem Tauri e sem rede.
import type { Account, Draft, Provider, MessageDetail, MessageSummary, NewAccount, Role, SendReport, SyncResult } from "./api";

const now = Math.floor(Date.now() / 1000);
const H = 3600;

const seed: [string, string, string, string, number][] = [
  ["Marina Duarte", "marina@lumenarquitetura.com.br", "Projeto da sala de reunião: revisão final", "Segue a planta com os ajustes de iluminação que conversamos. Se aprovar até sexta, a obra começa dia 16.", 0.4],
  ["Banco Inter", "noreply@inter.co", "Sua fatura de novembro fechou", "Valor total R$ 2.418,90, vencimento em 15/11. O pagamento pode ser feito pelo app ou por Pix.", 2],
  ["Rafael Lima", "rafael.lima@gmail.com", "Churrasco no sábado?", "Fala! Vamos marcar lá em casa às 13h. Leva só o carvão que o resto eu resolvo.", 5],
  ["GitHub", "noreply@github.com", "[posta] Nova issue: sincronização no Android", "A sincronização em segundo plano para quando o app sai da tela. Precisamos de um serviço em foreground.", 9],
  ["Juliana Prado", "juliana@prado.adv.br", "Contrato de locação assinado", "O contrato está assinado pelas duas partes. Envio a via digital em anexo e a física pelos Correios.", 20],
  ["Azul Linhas Aéreas", "naoresponda@voeazul.com.br", "Check-in disponível: VCP → SDU", "Seu voo AD 4321 sai amanhã às 07:40. Faça o check-in e escolha seu assento.", 26],
  ["Pedro Antunes", "pedro@antunes.dev", "Re: revisão do schema", "Concordo com o FTS5. Só cuidado com o tokenizer: remove_diacritics 2 resolve a busca sem acento.", 50],
  ["Nubank", "todomundo@nubank.com.br", "Pix recebido de Carla Mendes", "Você recebeu R$ 150,00 de Carla Mendes. Descrição: rateio do presente.", 73],
  ["Carla Mendes", "carla.mendes@outlook.com", "Fotos da viagem", "Separei as melhores fotos de Paraty, estão no link compartilhado. Dá uma olhada quando puder.", 98],
  ["Prefeitura de Campinas", "nfse@campinas.sp.gov.br", "Nota fiscal de serviço emitida", "A NFS-e nº 00012873 foi emitida. Consulte o documento pelo código de verificação no portal.", 140],
];

let accounts: Account[] = [{ id: "renan@imap.exemplo.com", host: "imap.exemplo.com", port: 993, security: "tls", username: "renan@exemplo.com" }];

type MockMessage = MessageDetail & { role: Role };

let messages: MockMessage[] = Array.from({ length: 60 }, (_, i) => {
  const [from_name, from_addr, subject, body, hoursAgo] = seed[i % seed.length];
  const round = Math.floor(i / seed.length);
  return {
    id: i + 1,
    account: accounts[0].id,
    uid: i + 1,
    from_name,
    from_addr,
    subject: round ? `${subject} (${round + 1})` : subject,
    date: now - Math.round((hoursAgo + round * 200) * H),
    snippet: body.slice(0, 200),
    body_text: `${body}\n\nAbraço,\n${from_name.split(" ")[0]}`,
    body_html: "",
    seen: i > 2 && i % 4 !== 0,
    reply_addr: from_addr,
    attachments: [],
    role: "inbox",
  };
});

// Dois emails em HTML: um simples e outro com imagem remota (para a faixa "Mostrar imagens").
const IMG = "https://placehold.co/560x180/png?text=Oferta+de+inverno";
const htmlSamples: [string, string, string, string, string, number][] = [
  ["Café Origem", "contato@cafeorigem.com.br", "Seu pedido #4821 saiu para entrega", "Seu pedido saiu para entrega e chega amanhã entre 9h e 12h.",
    `<div style="font-family:Georgia,serif;max-width:560px;margin:0 auto;padding:16px"><h2 style="color:#5b3a1e">Pedido a caminho</h2><p>Olá, Renan! Seu pedido <b>#4821</b> saiu para entrega e chega amanhã entre <b>9h e 12h</b>.</p><table style="width:100%;border-collapse:collapse"><tr><td style="padding:6px 0;border-bottom:1px solid #eee">Café Santa Luzia 250 g</td><td style="text-align:right;border-bottom:1px solid #eee">R$ 38,00</td></tr><tr><td style="padding:6px 0">Frete</td><td style="text-align:right">R$ 12,00</td></tr></table><p><a href="https://example.com/rastreio">Acompanhar entrega</a></p></div>`, 1.2],
  ["Loja Norte", "ofertas@lojanorte.com.br", "Últimos dias: casacos com 40%", "Casacos, botas e cachecóis com até 40% de desconto.",
    `<div style="font-family:Arial,sans-serif;max-width:560px;margin:0 auto"><img src="${IMG}" width="560" alt="Oferta de inverno" style="display:block;width:100%"><div style="padding:16px"><h2 style="margin:0 0 8px">Frio chegou, desconto também</h2><p>Casacos, botas e cachecóis com até <b>40% de desconto</b> até domingo.</p><p><a href="https://example.com/ofertas" style="background:#111;color:#fff;padding:10px 16px;border-radius:6px;text-decoration:none">Ver ofertas</a></p></div></div>`, 3.5],
];
htmlSamples.forEach(([from_name, from_addr, subject, text, html, hoursAgo], i) => {
  const id = 1000 + i;
  messages.unshift({
    id, account: accounts[0].id, uid: id, from_name, from_addr, subject,
    date: now - Math.round(hoursAgo * H), snippet: text, body_text: text, body_html: html,
    seen: false, reply_addr: from_addr, attachments: [], role: "inbox",
  });
});

// Mensagem com anexos (e uma imagem inline `cid:` já embutida como data: URL, como o núcleo faz).
const LOGO = "data:image/svg+xml;utf8," + encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="160" height="48"><rect width="160" height="48" rx="8" fill="#1f6c9f"/><text x="80" y="31" font-family="sans-serif" font-size="20" fill="#fff" text-anchor="middle">Lumen</text></svg>`);
messages.unshift({
  id: 1100, account: accounts[0].id, uid: 1100, from_name: "Marina Duarte", from_addr: "marina@lumenarquitetura.com.br",
  subject: "Planta e orçamento da sala de reunião", date: now - Math.round(0.2 * H),
  snippet: "Seguem a planta revisada e o orçamento em anexo.", body_text: "Seguem a planta revisada e o orçamento em anexo.\n\nMarina",
  body_html: `<div style="font-family:Arial,sans-serif;max-width:560px"><img src="${LOGO}" width="160" height="48" alt="Lumen"><p>Oi, Renan! Seguem a <b>planta revisada</b> e o orçamento em anexo.</p><p>Marina</p></div>`,
  seen: false, reply_addr: "marina@lumenarquitetura.com.br", role: "inbox",
  attachments: [
    { name: "planta-sala-reuniao-v3.pdf", mime: "application/pdf", size: 2_516_582, index: 0 },
    { name: "orçamento.xlsx", mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", size: 48_210, index: 1 },
    { name: "foto-obra.jpg", mime: "image/jpeg", size: 874_112, index: 2 },
  ],
});

// Enviados e Lixeira de exemplo.
const extra: [Role, string, string, string, string][] = [
  ["sent", "Marina Duarte", "marina@lumenarquitetura.com.br", "Re: Projeto da sala de reunião", "Aprovado! Pode seguir com a obra na data combinada."],
  ["sent", "Rafael Lima", "rafael.lima@gmail.com", "Re: Churrasco no sábado?", "Fechado, levo o carvão e a sobremesa."],
  ["sent", "Juliana Prado", "juliana@prado.adv.br", "Dúvida sobre o contrato", "Preciso confirmar a cláusula sobre reajuste antes de assinar."],
  ["trash", "Promoções Loja", "ofertas@loja.com.br", "Últimas horas: 40% de desconto", "Aproveite as ofertas de fim de semana em toda a loja."],
  ["trash", "Newsletter Semanal", "news@semanal.com", "Resumo da semana", "Os destaques da semana em tecnologia e negócios."],
];
extra.forEach(([role, name, addr, subject, body], i) => {
  const id = messages.length + 1;
  const mine = role === "sent";
  messages.push({
    id, account: accounts[0].id, uid: id,
    from_name: mine ? "Renan" : name, from_addr: mine ? "renan@exemplo.com" : addr,
    subject, date: now - (i + 1) * 30 * H, snippet: body, body_text: `${body}\n\n${mine ? "Renan" : name}`, body_html: "",
    seen: true, reply_addr: addr, attachments: [], role,
  });
});

const norm = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
const summary = ({ body_text: _b, body_html: _h, reply_addr: _r, attachments: _a, role: _o, ...s }: MockMessage): MessageSummary => s;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
let syncCount = 0;

export const mockApi = {
  async listAccounts(): Promise<Account[]> {
    return accounts;
  },
  async addAccount(a: NewAccount): Promise<Account> {
    await wait(900);
    if (a.password.length < 3) throw "LOGIN failed. Invalid login/password";
    const acc = { id: `${a.username}@${a.host}`, host: a.host, port: a.port, security: a.security, username: a.username, smtp: a.smtp };
    accounts = [...accounts.filter((x) => x.id !== acc.id), acc];
    return acc;
  },
  async oauthProviders(): Promise<Provider[]> {
    return ["google", "microsoft"];
  },
  async oauthLogin(provider: Provider): Promise<Account> {
    await wait(1200);
    const host = provider === "google" ? "imap.gmail.com" : "outlook.office365.com";
    const acc: Account = { id: `voce@${host}`, host, port: 993, security: "tls", username: "voce@exemplo.com", auth: provider };
    accounts = [...accounts.filter((x) => x.id !== acc.id), acc];
    return acc;
  },
  async folders(): Promise<Role[]> {
    return ["inbox", "sent", "archive", "trash", "junk"];
  },
  async syncFolder(role: Role, _keep: number): Promise<SyncResult> {
    await wait(500);
    return { fetched: 0, total: messages.filter((m) => m.role === role).length, errors: [], skipped: false };
  },
  async sync(): Promise<SyncResult> {
    await wait(700);
    syncCount += 1;
    if (syncCount % 3 !== 2) return { fetched: 0, total: messages.length, errors: [], skipped: false };
    const id = messages.length + 1;
    messages = [
      {
        id, account: accounts[0].id, uid: id, from_name: "Marina Duarte", from_addr: "marina@lumenarquitetura.com.br",
        subject: "Orçamento aprovado", date: Math.floor(Date.now() / 1000),
        snippet: "Tudo certo com o orçamento. Podemos assinar na segunda?", body_text: "Tudo certo com o orçamento. Podemos assinar na segunda?\n\nMarina", body_html: "", seen: false,
        reply_addr: "marina@lumenarquitetura.com.br", attachments: [], role: "inbox",
      },
      ...messages,
    ];
    return { fetched: 1, total: messages.length, errors: [], skipped: false };
  },
  async list(role: Role, limit: number, offset: number): Promise<MessageSummary[]> {
    return messages.filter((m) => m.role === role).sort((a, b) => b.date - a.date).slice(offset, offset + limit).map(summary);
  },
  async search(query: string, role: Role): Promise<MessageSummary[]> {
    const terms = norm(query).split(/\s+/).filter(Boolean);
    return messages
      .filter((m) => m.role === role && terms.every((t) => norm(`${m.subject} ${m.from_name} ${m.from_addr} ${m.body_text}`).includes(t)))
      .sort((a, b) => b.date - a.date)
      .map(summary);
  },
  async open(id: number): Promise<MessageDetail | null> {
    const m = messages.find((x) => x.id === id);
    if (!m) return null;
    m.seen = true;
    return { ...m };
  },
  /** Para testar a falha: assunto com "fatura" não move. */
  async move(id: number, role: Role): Promise<void> {
    await wait(400);
    const m = messages.find((x) => x.id === id);
    if (!m) throw "mensagem não encontrada";
    if (m.subject.toLowerCase().includes("fatura")) throw "IMAP: servidor recusou o MOVE";
    m.role = role;
  },
  async deleteForever(id: number): Promise<void> {
    await wait(400);
    messages = messages.filter((x) => x.id !== id);
  },
  /** Para testar a falha: anexo com "falha" no nome. */
  async saveAttachment(id: number, index: number, _open: boolean): Promise<string> {
    await wait(500);
    const a = messages.find((x) => x.id === id)?.attachments.find((x) => x.index === index);
    if (!a) throw "anexo não encontrado na mensagem";
    if (a.name.includes("falha")) throw "IMAP: conexão recusada";
    return `/home/renan/Downloads/${a.name}`;
  },
  /** Para testar a falha: qualquer destinatário com "erro". */
  async send(d: Draft): Promise<SendReport> {
    await wait(800);
    if (d.to.includes("erro")) throw "SMTP: connection refused";
    return { saved_to_sent: true, warning: null };
  },
};
