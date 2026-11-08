// Dados de exemplo para rodar a UI no navegador (pnpm dev), sem Tauri e sem rede.
import type { Account, Draft, MessageDetail, MessageSummary, NewAccount, SendReport, SyncResult } from "./api";

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

let messages: MessageDetail[] = Array.from({ length: 60 }, (_, i) => {
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
    seen: i > 2 && i % 4 !== 0,
    reply_addr: from_addr,
  };
});

const norm = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
const summary = ({ body_text: _b, reply_addr: _r, ...s }: MessageDetail): MessageSummary => s;
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
  async sync(): Promise<SyncResult> {
    await wait(700);
    syncCount += 1;
    if (syncCount % 3 !== 2) return { fetched: 0, errors: [], skipped: false };
    const id = messages.length + 1;
    messages = [
      {
        id, account: accounts[0].id, uid: id, from_name: "Marina Duarte", from_addr: "marina@lumenarquitetura.com.br",
        subject: "Orçamento aprovado", date: Math.floor(Date.now() / 1000),
        snippet: "Tudo certo com o orçamento. Podemos assinar na segunda?", body_text: "Tudo certo com o orçamento. Podemos assinar na segunda?\n\nMarina", seen: false,
        reply_addr: "marina@lumenarquitetura.com.br",
      },
      ...messages,
    ];
    return { fetched: 1, errors: [], skipped: false };
  },
  async list(limit: number, offset: number): Promise<MessageSummary[]> {
    return [...messages].sort((a, b) => b.date - a.date).slice(offset, offset + limit).map(summary);
  },
  async search(query: string): Promise<MessageSummary[]> {
    const terms = norm(query).split(/\s+/).filter(Boolean);
    return messages
      .filter((m) => terms.every((t) => norm(`${m.subject} ${m.from_name} ${m.from_addr} ${m.body_text}`).includes(t)))
      .sort((a, b) => b.date - a.date)
      .map(summary);
  },
  async open(id: number): Promise<MessageDetail | null> {
    const m = messages.find((x) => x.id === id);
    if (!m) return null;
    m.seen = true;
    return { ...m };
  },
  /** Para testar a falha: qualquer destinatário com "erro". */
  async send(d: Draft): Promise<SendReport> {
    await wait(800);
    if (d.to.includes("erro")) throw "SMTP: connection refused";
    return { saved_to_sent: true, warning: null };
  },
};
