// Dados de exemplo para rodar a UI no navegador (pnpm dev), sem Tauri e sem rede.
import type { Account, CheckFailure, CheckStep, Detection, FailKind, Contact, Draft, FlushReport, MailDelta, MessagePage, OutboxItem, QueueStatus, Provider, OauthConfig, OauthConfigView, MessageDetail, MessageSummary, NewAccount, OutAttachment, Role, ScheduledEdit, SendReport, Settings, StoredDraft, SyncResult, UnreadAll, UserFolder, View, DragFiles, FolderUnread } from "./api";
import { parseRules, type ScheduledItem } from "./productivity";
import { parseBox, parseCat } from "./views";

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

let accounts: Account[] = [
  { id: "renan@imap.exemplo.com", host: "imap.exemplo.com", port: 993, security: "tls", username: "renan@exemplo.com", name: "Renan Santos" },
  { id: "contato@imap.betinhos.com.br", host: "imap.betinhos.com.br", port: 993, security: "tls", username: "contato@betinhos.com.br", name: "Betinhos" },
];

/** `thread_count` é calculado ao responder; `thread_id` 0 = mensagem sem fio. `box`: nome completo da pasta do usuário (então `role` é null). */
type MockMessage = Omit<MessageDetail, "thread_count"> & { role: Role | null; box?: string; snoozeUntil?: number; woke?: boolean; noBody?: boolean };

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
    flagged: i % 7 === 2,
    reply_addr: from_addr,
    attachments: [],
    to_addrs: "renan@exemplo.com",
    cc_addrs: i % 3 === 0 ? "carla.mendes@outlook.com, pedro@antunes.dev" : "",
    role: "inbox",
    thread_id: 0,
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
    seen: false, flagged: false, reply_addr: from_addr, attachments: [], to_addrs: "renan@exemplo.com", cc_addrs: "", role: "inbox", thread_id: 0,
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
  seen: false, flagged: true, reply_addr: "marina@lumenarquitetura.com.br", role: "inbox", thread_id: 1100,
  to_addrs: "contato@exemplo.com", cc_addrs: "carla.mendes@outlook.com, pedro@antunes.dev",
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
    seen: true, flagged: false, reply_addr: addr, attachments: [], to_addrs: mine ? addr : "renan@exemplo.com", cc_addrs: "", role, thread_id: 0,
  });
});

// Conversas: o fio da Marina (3 mensagens dela e a minha resposta em Enviados) e um fio da segunda conta.
const second = accounts[1].id;
const thread = (id: number, account: string, role: Role, thread_id: number, subject: string, from: [string, string], text: string, hoursAgo: number, seen: boolean, to: string) =>
  messages.push({
    id, account, uid: id, from_name: from[0], from_addr: from[1], subject, date: now - Math.round(hoursAgo * H),
    snippet: text.slice(0, 200), body_text: `${text}\n\n${from[0].split(" ")[0]}`, body_html: "", seen, flagged: false,
    reply_addr: from[1], attachments: [], to_addrs: to, cc_addrs: "", role, thread_id,
  });
const MARINA: [string, string] = ["Marina Duarte", "marina@lumenarquitetura.com.br"];
const ME: [string, string] = ["Renan", "renan@exemplo.com"];
thread(1101, accounts[0].id, "inbox", 1100, "Planta e orçamento da sala de reunião", MARINA, "Oi, Renan! Posso te mandar a planta da sala de reunião ainda hoje? Preciso só do teu ok no orçamento.", 30, true, "renan@exemplo.com");
thread(1102, accounts[0].id, "sent", 1100, "Re: Planta e orçamento da sala de reunião", ME, "Pode mandar sim. O orçamento fecha se o prazo de entrega for até o dia 16.", 26, true, MARINA[1]);
thread(1103, accounts[0].id, "inbox", 1100, "Re: Planta e orçamento da sala de reunião", MARINA, "Fechado, o prazo é dia 16. Estou revisando a iluminação e já te envio a versão final.", 6, true, "renan@exemplo.com");
const HELENA: [string, string] = ["Helena Bastos", "helena@cliente-alfa.com.br"];
const CONTATO = "contato@betinhos.com.br";
thread(2001, second, "inbox", 2001, "Transfer aeroporto: sexta às 18h", HELENA, "Preciso de um transfer de Congonhas para o hotel na sexta, 18h, para 2 passageiros.", 22, true, CONTATO);
thread(2002, second, "sent", 2001, "Re: Transfer aeroporto: sexta às 18h", ["Betinhos", CONTATO], "Olá, Helena! Confirmado: sedan executivo, motorista com placa de recepção. Valor R$ 280,00.", 21, true, HELENA[1]);
thread(2003, second, "inbox", 2001, "RES: Transfer aeroporto: sexta às 18h", HELENA, "Perfeito. Pode incluir também a volta no domingo, às 16h?", 3, false, CONTATO);
thread(2004, second, "inbox", 0, "Renovação do seguro frota", ["Porto Seguro", "frota@portoseguro.com.br"], "A renovação da apólice da frota vence em 30 dias. Veja as condições no anexo.", 8, false, CONTATO);
thread(2005, second, "inbox", 0, "Nota fiscal de combustível", ["Posto Ipiranga", "nf@postoipiranga.com.br"], "Sua nota fiscal de combustível de outubro está disponível.", 52, true, CONTATO);

// Rascunhos e configurações de exemplo (a tela de Rascunhos e as assinaturas aparecem preenchidas).
let drafts: StoredDraft[] = [
  { id: "d-seed-1", account: accounts[0].id, reply_to: null, to: "marina@lumenarquitetura.com.br", subject: "Cronograma da obra", body: "Marina, segue o cronograma revisado. Podemos fechar na segunda?", updated: now - 2 * H },
  { id: "d-seed-2", account: accounts[0].id, reply_to: null, to: "", subject: "", body: "Lembrar de cobrar o orçamento do marceneiro.", updated: now - 30 * H },
];
// As preferências sobrevivem ao reload (como no app, que grava no banco local).
let autostart = false;
const SETTINGS_KEY = "posta-mock-settings";
let settings: Settings = { [`signature:${accounts[0].id}`]: "Renan\nBetinhos Executive Service", [`aliases:${accounts[0].id}`]: "Betinhos Executive <contato@exemplo.com>\nreservas@exemplo.com" };
try { settings = { ...settings, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") }; } catch { /* sem armazenamento */ }

// Pastas do usuário de exemplo (separador "/"), com duas mensagens na subpasta.
let userFolders: UserFolder[] = ["Clientes", "Clientes/Lumen", "Clientes/Lumen/Obras", "Projetos"].map((name) => ({ account: accounts[0].id, name, delimiter: "/" }));

/** `?contas=5` no endereço acrescenta 3 contas (com mensagens e pastas) e `?contas=1` deixa só a primeira, para testar a navegação; o padrão segue com 2. */
const mockContas = typeof location !== "undefined" ? new URLSearchParams(location.search).get("contas") : null;
if (mockContas === "1") {
  accounts = accounts.slice(0, 1);
  messages = messages.filter((m) => m.account === accounts[0].id);
}
if (mockContas === "5") {
  const more: [string, string, string, string][] = [
    ["imap.gmail.com", "renan@gmail.com", "Renan (Gmail)", "Pessoal"],
    ["imap.betinhos.com.br", "financeiro@betinhos.com.br", "Financeiro", "Boletos"],
    ["imap.betinhos.com.br", "marketing@betinhos.com.br", "Marketing", "Campanhas"],
  ];
  more.forEach(([host, username, name, folder], k) => {
    const id = `${username.split("@")[0]}@${host}`;
    accounts.push({ id, host, port: 993, security: "tls", username, name });
    userFolders.push({ account: id, name: folder, delimiter: "/" });
    for (let i = 0; i < 4; i++) {
      const [from_name, from_addr, subject, body, hoursAgo] = seed[(k * 3 + i) % seed.length];
      const mid = 5000 + k * 10 + i;
      messages.push({
        id: mid, account: id, uid: mid, from_name, from_addr, subject, date: now - Math.round((hoursAgo + k) * H),
        snippet: body.slice(0, 200), body_text: `${body}\n\n${from_name.split(" ")[0]}`, body_html: "",
        seen: i === 3, flagged: i === 1, reply_addr: from_addr, attachments: [], to_addrs: username, cc_addrs: "", role: "inbox", thread_id: 0,
      });
    }
  });
}
messages.filter((m) => m.id === 1 || m.id === 5).forEach((m) => { m.role = null; m.box = "Clientes/Lumen"; });
const inTree = (m: MockMessage, name: string) => !!m.box && (m.box === name || m.box.startsWith(`${name}/`));
const leaf = (name: string) => {
  const t = name.trim();
  if (!t || /[/*%\\]/.test(t)) throw "Nome inválido: não use / * % nem deixe vazio";
  return t;
};

const norm = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
const withCount = <T extends MockMessage>(m: T) => ({
  ...m,
  thread_count: m.thread_id ? messages.filter((x) => x.account === m.account && x.thread_id === m.thread_id).length : 1,
});
const summary = (m: MockMessage): MessageSummary => {
  const { box: _x, snoozeUntil: _s, woke: _w, noBody: _n, body_text: _b, body_html: _h, reply_addr: _r, attachments: _a, to_addrs: _t, cc_addrs: _c, ...s } = withCount(m);
  return s;
};
/** Pastas virtuais: "Com estrela" (fora de Lixeira e Spam), "Todas as caixas" (a Entrada de todas as contas), pastas do usuário e categorias. */
const snoozed = (m: MockMessage) => (m.snoozeUntil ?? 0) > Math.floor(Date.now() / 1000);
/** Ordem da lista: fixadas primeiro; depois a data (ou a hora em que a adiada voltou). */
const topKey = (m: MockMessage) => Math.max(m.date, m.snoozeUntil ?? 0);
const byTop = (a: MockMessage, b: MockMessage) => Number(!!b.pinned) - Number(!!a.pinned) || topKey(b) - topKey(a) || b.id - a.id;
/** Adiadas que venceram voltam como não lidas. */
const wake = () => {
  for (const m of messages) if (m.snoozeUntil && !snoozed(m) && !m.woke) { m.woke = true; m.seen = false; }
};
const inView = (m: MockMessage, view: View) => {
  if (snoozed(m)) return false;
  const box = parseBox(view);
  const cat = parseCat(view);
  if (box) return m.account === box.account && m.box === box.name;
  if (cat) return m.category === cat && m.role !== "trash" && m.role !== "junk";
  return view === "starred" ? m.flagged && m.role !== "trash" && m.role !== "junk" : m.role === (view === "all" ? "inbox" : view);
};
/**
 * Pasta ordenada para a paginação. Reordenar 50 mil mensagens a cada página custaria mais que o app inteiro, então guarda o
 * resultado e só refaz quando a assinatura (quem está na pasta e quem está fixada) muda.
 */
let viewCache: { ref: MockMessage[]; folder: Role; sig: string; list: MockMessage[] } | null = null;
function sortedView(folder: Role): MockMessage[] {
  let n = 0, sum = 0, pins = 0, pinSum = 0;
  for (const m of messages) {
    if (!inView(m, folder)) continue;
    n++; sum += m.id;
    if (m.pinned) { pins++; pinSum += m.id; }
  }
  const sig = `${n}:${sum}:${pins}:${pinSum}`;
  if (viewCache && viewCache.ref === messages && viewCache.folder === folder && viewCache.sig === sig) return viewCache.list;
  const list = messages.filter((m) => inView(m, folder)).sort(byTop);
  viewCache = { ref: messages, folder, sig, list };
  return list;
}
/** Mesmo vocabulário do núcleo: de:, para:, assunto:, tem:anexo, é:não-lida, antes:, depois:; o resto é texto livre. */
function matchFilters(m: MockMessage, query: string): boolean {
  const text: string[] = [];
  const ok = (v: string, hay: string) => norm(hay).includes(norm(v));
  let pass = true;
  for (const t of query.match(/(?:[^\s"]+|"[^"]*")+/g) ?? []) {
    const [key, ...rest] = t.replace(/"/g, "").split(":");
    const val = rest.join(":");
    const k = norm(key);
    const v = norm(val);
    const day = /^\d{4}-\d{2}-\d{2}$/.test(val) ? Date.parse(`${val}T00:00:00-03:00`) / 1000 : NaN;
    if (!val) text.push(t);
    else if (k === "de" || k === "from") pass &&= ok(val, `${m.from_name} ${m.from_addr}`);
    else if (k === "para" || k === "to") pass &&= ok(val, `${m.to_addrs} ${m.cc_addrs}`);
    else if (k === "assunto" || k === "subject") pass &&= ok(val, m.subject);
    else if ((k === "tem" || k === "has") && v.startsWith("anexo")) pass &&= m.attachments.length > 0;
    else if ((k === "e" || k === "is") && (v === "nao-lida" || v === "naolida")) pass &&= !m.seen;
    else if ((k === "e" || k === "is") && v === "lida") pass &&= m.seen;
    else if ((k === "antes" || k === "before") && !isNaN(day)) pass &&= m.date < day;
    else if ((k === "depois" || k === "after" || k === "desde") && !isNaN(day)) pass &&= m.date >= day;
    else text.push(t);
  }
  return pass && text.every((w) => ok(w, `${m.subject} ${m.from_name} ${m.from_addr} ${m.body_text}`));
}

/** Mensagens que só existem "no servidor": a busca no servidor as traz para o banco local. */
const serverOnly: MockMessage[] = [
  ["Marina Duarte", "marina@lumenarquitetura.com.br", "Contrato de prestação 2023 (arquivo antigo)", 9000],
  ["Juliana Prado", "juliana@prado.adv.br", "Contrato social: versão final de 2022", 15000],
].map(([from_name, from_addr, subject, hoursAgo], i) => ({
  id: 5000 + i, account: accounts[0].id, uid: 5000 + i, from_name: String(from_name), from_addr: String(from_addr), subject: String(subject),
  date: now - Number(hoursAgo) * H, snippet: "Mensagem antiga, só no servidor.", body_text: "Mensagem antiga, só no servidor.", body_html: "",
  seen: true, flagged: false, reply_addr: String(from_addr), attachments: [], to_addrs: "renan@exemplo.com", cc_addrs: "", role: "inbox" as Role, thread_id: 0,
}));

/** Agendados do mock; a mensagem sai quando a hora chega (como o relógio do app). */
let scheduled: { item: ScheduledItem; draft: Draft }[] = [];
setInterval(() => {
  const t = Math.floor(Date.now() / 1000);
  const due = scheduled.filter((s) => s.item.send_at <= t);
  if (!due.length) return;
  scheduled = scheduled.filter((s) => s.item.send_at > t);
  for (const { draft } of due) {
    const id = Math.max(...messages.map((m) => m.id)) + 1;
    messages.push({
      id, account: draft.account, uid: id, from_name: "Renan", from_addr: "renan@exemplo.com", subject: draft.subject, date: t,
      snippet: draft.body.slice(0, 200), body_text: draft.body, body_html: "", seen: true, flagged: false, reply_addr: draft.to,
      attachments: [], to_addrs: draft.to, cc_addrs: "", role: "sent", thread_id: 0,
    });
  }
}, 2000);

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Rede simulada: no console do navegador, `__mockOffline(true)` liga o modo offline (ações entram na fila).
let offline = false;
let pendingOps = 0;
let outbox: OutboxItem[] = [];
(window as unknown as { __mockOffline: (v: boolean) => void }).__mockOffline = (v) => {
  offline = v;
  window.dispatchEvent(new Event(v ? "offline" : "online"));
};
// Teste de carga: `__mockMany(50000)` recria a Entrada com n mensagens (datas espalhadas por ~3 anos) e avisa a UI.
(window as unknown as { __mockMany: (n: number) => void }).__mockMany = (n) => {
  const base = messages.filter((m) => m.id >= 1000);
  const gen: MockMessage[] = Array.from({ length: n }, (_, i) => {
    const [from_name, from_addr, subject, body] = seed[i % seed.length];
    return {
      id: 10_000 + i, account: accounts[0].id, uid: 10_000 + i, from_name, from_addr,
      subject: `${subject} #${i + 1}`, date: now - Math.round(i * (3 * 365 * 24 * 60 * 60 / Math.max(n, 1))),
      snippet: body.slice(0, 200), body_text: body, body_html: "", seen: i % 5 !== 0, flagged: i % 11 === 0,
      reply_addr: from_addr, attachments: [], to_addrs: "renan@exemplo.com", cc_addrs: "", role: "inbox", thread_id: 0,
      noBody: i % 10 === 7,
    };
  });
  messages = [...base, ...gen];
  window.dispatchEvent(new Event("mock-changed"));
};
/** Chamadas de leitura da lista, para conferir no console/teste que a UI pagina e aplica delta em vez de recarregar tudo. */
const calls = { listPage: 0, list: 0, summaries: 0 };
(window as unknown as { __mockCalls: typeof calls }).__mockCalls = calls;
/** Equivalente ao evento `mail-delta` do backend: a UI aplica por id em vez de recarregar. */
const emitDelta = (d: Partial<MailDelta>) =>
  window.dispatchEvent(new CustomEvent<MailDelta>("mock-delta", { detail: { account: accounts[0].id, mailbox: "INBOX", inserted: [], changed: [], removed: [], ...d } }));
/** Chega `n` emails novos na Entrada (mais novos que tudo) e avisa por delta. Console: `__mockArrive(3)`. */
const arrive = (n = 1): number[] => {
  const top = Math.max(...messages.map((m) => m.id), 0);
  const t = Math.floor(Date.now() / 1000);
  const fresh: MockMessage[] = Array.from({ length: n }, (_, i) => ({
    id: top + 1 + i, account: accounts[0].id, uid: top + 1 + i, from_name: "Marina Duarte", from_addr: "marina@lumenarquitetura.com.br",
    subject: "Orçamento aprovado", date: t + i, snippet: "Tudo certo com o orçamento. Podemos assinar na segunda?",
    body_text: "Tudo certo com o orçamento. Podemos assinar na segunda?\n\nMarina", body_html: "", seen: false, flagged: false,
    reply_addr: "marina@lumenarquitetura.com.br", attachments: [], to_addrs: "renan@exemplo.com", cc_addrs: "", role: "inbox", thread_id: 0,
  }));
  messages = [...fresh, ...messages];
  const ids = fresh.map((m) => m.id);
  emitDelta({ inserted: ids });
  return ids;
};
(window as unknown as { __mockArrive: (n?: number) => number[] }).__mockArrive = arrive;
const queue = (n: number) => { if (offline) pendingOps += n; };
let syncCount = 0;
let oauthConfig: OauthConfig = { google_id: "", google_secret: "", microsoft_id: "" };

export const mockApi = {
  async listAccounts(): Promise<Account[]> {
    return accounts;
  },
  async addAccount(a: NewAccount): Promise<Account> {
    await wait(900);
    if (a.password.length < 3) throw "LOGIN failed. Invalid login/password";
    const acc = { id: `${a.username}@${a.host}`, host: a.host, port: a.port, security: a.security, username: a.username, smtp: a.smtp, name: a.name };
    accounts = [...accounts.filter((x) => x.id !== acc.id), acc];
    return acc;
  },
  /** Domínios de teste: empresa.com.br (MX Microsoft), workspace.com.br (MX Google), exemplo.org (autoconfig), offline.com (sem resposta); o resto não é encontrado. */
  async detectProvider(domain: string): Promise<Detection> {
    await wait(700);
    if (domain === "empresa.com.br") return { imap: null, smtp: null, hosting: "microsoft" };
    if (domain === "workspace.com.br") return { imap: null, smtp: null, hosting: "google" };
    if (domain === "exemplo.org") return { imap: { host: "mail.exemplo.org", port: 143, security: "starttls" }, smtp: { host: "smtp.exemplo.org", port: 587, security: "starttls" }, hosting: "other" };
    if (domain === "offline.com") await wait(7000);
    return { imap: null, smtp: null, hosting: null };
  },
  /** Para testar as falhas: servidor com "invalido" (DNS), "lento" (prazo), porta 1234 (recusada), "tls" (certificado), senha "errada" ou "imap" (IMAP desativado). */
  async checkAccount(a: NewAccount, step: CheckStep): Promise<void> {
    await wait(step === "connect" ? 600 : 800);
    const host = step === "smtp" ? a.smtp?.host ?? "" : a.host;
    const port = step === "smtp" ? a.smtp?.port : a.port;
    const bad = (kind: FailKind, detail: string) => Promise.reject({ step, kind, detail } satisfies CheckFailure);
    if (host.includes("invalido")) return bad("dns", `failed to lookup address information: Name or service not known (${host})`);
    if (host.includes("lento")) return bad("timeout", "sem resposta do servidor no prazo");
    if (port === 1234) return bad("refused", "Connection refused (os error 111)");
    if (host.includes("tls")) return bad("tls", "invalid peer certificate: UnknownIssuer");
    if (step === "login" && a.password === "errada") return bad("auth", "No Response: [AUTHENTICATIONFAILED] Invalid credentials (Failure)");
    if (step === "login" && a.password === "imap") return bad("imap_disabled", "No Response: [ALERT] Your account is not enabled for IMAP use.");
  },
  async oauthProviders(): Promise<Provider[]> {
    return ["google", "microsoft"];
  },
  async oauthRelogin(_id: string): Promise<void> {
    await wait(1000);
  },
  async oauthConfigGet(): Promise<OauthConfigView> {
    return { ...oauthConfig, built_in_google: false, built_in_microsoft: false };
  },
  async oauthConfigSet(config: OauthConfig): Promise<void> {
    oauthConfig = config;
  },
  async oauthLogin(provider: Provider): Promise<Account> {
    await wait(1200);
    const host = provider === "google" ? "imap.gmail.com" : "outlook.office365.com";
    const acc: Account = { id: `voce@${host}`, host, port: 993, security: "tls", username: "voce@exemplo.com", auth: provider };
    accounts = [...accounts.filter((x) => x.id !== acc.id), acc];
    return acc;
  },
  async removeAccount(id: string): Promise<void> {
    await wait(400);
    accounts = accounts.filter((a) => a.id !== id);
    drafts = drafts.filter((d) => d.account !== id);
    messages = messages.filter((m) => m.account !== id);
    delete settings[`signature:${id}`];
    delete settings[`color:${id}`];
  },
  async saveDraft(d: Omit<StoredDraft, "updated">): Promise<void> {
    drafts = [{ ...d, updated: Math.floor(Date.now() / 1000) }, ...drafts.filter((x) => x.id !== d.id)];
  },
  async listDrafts(): Promise<StoredDraft[]> {
    return [...drafts].sort((a, b) => b.updated - a.updated);
  },
  /** Mesma regra do núcleo: remetentes, Para e Cc; mais frequentes primeiro, empate pelo mais recente. */
  async suggestContacts(query: string, limit: number): Promise<Contact[]> {
    const q = norm(query.trim());
    const found = new Map<string, Contact>();
    for (const m of [...messages].sort((a, b) => b.date - a.date)) {
      const seen: [string, string][] = [[m.from_name, m.from_addr], ...[...m.to_addrs.split(","), ...m.cc_addrs.split(",")].map((a): [string, string] => ["", a.trim()])];
      for (const [name, addr] of seen) {
        if (!addr.includes("@")) continue;
        const c = found.get(addr.toLowerCase()) ?? { name: "", addr, count: 0 };
        c.count++;
        c.name ||= name;
        found.set(addr.toLowerCase(), c);
      }
    }
    return [...found.values()].filter((c) => !q || norm(`${c.name} ${c.addr}`).includes(q))
      .sort((a, b) => b.count - a.count).slice(0, limit);
  },
  async deleteDraft(id: string): Promise<void> {
    drafts = drafts.filter((d) => d.id !== id);
  },
  async settings(): Promise<Settings> {
    return { ...settings };
  },
  async setSetting(key: string, value: string): Promise<void> {
    settings = { ...settings, [key]: value };
    try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); } catch { /* sem armazenamento */ }
  },
  async setFocused(_focused: boolean): Promise<void> {},
  async saveEml(_id: number): Promise<string> {
    return "/home/renan/Downloads/mensagem.eml";
  },
  async getAutostart(): Promise<boolean> {
    return autostart;
  },
  async setAutostart(enabled: boolean): Promise<void> {
    autostart = enabled;
  },
  async requestNotifyPermission(): Promise<boolean> {
    return true;
  },
  async folders(): Promise<Role[]> {
    return ["inbox", "sent", "archive", "trash", "junk"];
  },
  async syncFolder(view: View, _keep: number): Promise<SyncResult> {
    await wait(500);
    if (offline) return { fetched: 0, total: 0, errors: [], skipped: false, offline: true };
    return { fetched: 0, total: messages.filter((m) => inView(m, view)).length, errors: [], skipped: false, offline: false };
  },
  async userFolders(): Promise<UserFolder[]> {
    return userFolders.filter((f) => accounts.some((a) => a.id === f.account));
  },
  /** Para testar a falha: nome "Falha". */
  async createFolder(account: string, parent: string | null, name: string): Promise<UserFolder[]> {
    await wait(400);
    const full = parent ? `${parent}/${leaf(name)}` : leaf(name);
    if (full.endsWith("Falha")) throw "IMAP: servidor recusou o CREATE";
    if (userFolders.some((f) => f.account === account && f.name.toLowerCase() === full.toLowerCase())) throw "Já existe uma pasta com esse nome";
    userFolders = [...userFolders, { account, name: full, delimiter: "/" }];
    return this.userFolders();
  },
  async renameFolder(account: string, folder: string, name: string): Promise<UserFolder[]> {
    await wait(400);
    const parent = folder.includes("/") ? folder.slice(0, folder.lastIndexOf("/") + 1) : "";
    const next = parent + leaf(name);
    if (userFolders.some((f) => f.account === account && f.name !== folder && f.name.toLowerCase() === next.toLowerCase())) throw "Já existe uma pasta com esse nome";
    const swap = (n: string) => (n === folder || n.startsWith(`${folder}/`) ? next + n.slice(folder.length) : n);
    userFolders = userFolders.map((f) => (f.account === account ? { ...f, name: swap(f.name) } : f));
    messages.forEach((m) => { if (m.account === account && m.box) m.box = swap(m.box); });
    return this.userFolders();
  },
  async countFolder(account: string, folder: string): Promise<number> {
    return messages.filter((m) => m.account === account && inTree(m, folder)).length;
  },
  async deleteFolder(account: string, folder: string): Promise<UserFolder[]> {
    await wait(400);
    userFolders = userFolders.filter((f) => !(f.account === account && (f.name === folder || f.name.startsWith(`${folder}/`))));
    messages = messages.filter((m) => !(m.account === account && inTree(m, folder)));
    return this.userFolders();
  },
  /** Devolve quantas foram movidas (só as da mesma conta da pasta). */
  async moveToFolder(ids: number[], account: string, folder: string): Promise<number> {
    await wait(400);
    const found = messages.filter((x) => ids.includes(x.id) && x.account === account && x.box !== folder);
    if (found.some((m) => m.subject.toLowerCase().includes("fatura"))) throw "IMAP: servidor recusou o MOVE";
    found.forEach((m) => { m.role = null; m.box = folder; });
    return found.length;
  },
  /** Copia sem tirar da origem; devolve quantas foram copiadas (só as da mesma conta da pasta). */
  async copyToFolder(ids: number[], account: string, folder: string): Promise<number> {
    await wait(400);
    const found = messages.filter((x) => ids.includes(x.id) && x.account === account && x.box !== folder);
    if (found.some((m) => m.subject.toLowerCase().includes("fatura"))) throw "IMAP: servidor recusou o COPY";
    const top = Math.max(...messages.map((m) => m.id));
    found.forEach((m, i) => messages.push({ ...m, id: top + 1 + i, role: null, box: folder }));
    return found.length;
  },
  async copyMany(ids: number[], role: Role): Promise<number> {
    await wait(400);
    const found = messages.filter((x) => ids.includes(x.id) && x.role !== role);
    const top = Math.max(...messages.map((m) => m.id));
    found.forEach((m, i) => messages.push({ ...m, id: top + 1 + i, role, box: undefined }));
    return found.length;
  },
  async userFolderUnread(): Promise<FolderUnread[]> {
    const out = new Map<string, FolderUnread>();
    for (const m of messages) {
      if (!m.box || m.seen) continue;
      const k = `${m.account}\u0000${m.box}`;
      out.set(k, { account: m.account, name: m.box, unread: (out.get(k)?.unread ?? 0) + 1 });
    }
    return [...out.values()];
  },
  async prepareDragEml(_ids: number[]): Promise<DragFiles> {
    throw "arrastar para fora só funciona no app instalado";
  },
  async prepareDragAttachment(_id: number, _index: number): Promise<DragFiles> {
    throw "arrastar para fora só funciona no app instalado";
  },
  async setCategory(ids: number[], category: string | null): Promise<void> {
    await wait(100);
    messages.forEach((m) => { if (ids.includes(m.id)) m.category = category; });
  },
  /** Para testar a falha: senha com menos de 3 caracteres ou servidor com "erro". */
  async updateAccount(a: Account, password: string | null): Promise<Account> {
    await wait(900);
    const cur = accounts.find((x) => x.id === a.id);
    if (!cur) throw "conta não encontrada";
    if (cur.auth && cur.auth !== "password") {
      cur.name = a.name?.trim() || null;
    } else {
      if (password !== null && password.length > 0 && password.length < 3) throw "LOGIN failed. Invalid login/password";
      if (a.host.includes("erro") || a.smtp?.host.includes("erro")) throw "conexão recusada pelo servidor";
      Object.assign(cur, { name: a.name?.trim() || null, host: a.host, port: a.port, security: a.security, smtp: a.smtp });
    }
    accounts = [...accounts];
    return { ...cur };
  },
  async sync(): Promise<SyncResult> {
    await wait(700);
    if (offline) return { fetched: 0, total: 0, errors: [], skipped: false, offline: true };
    syncCount += 1;
    if (syncCount % 3 !== 2) return { fetched: 0, total: messages.length, errors: [], skipped: false, offline: false };
    arrive(1);
    return { fetched: 1, total: messages.length, errors: [], skipped: false, offline: false };
  },
  async list(view: View, limit: number, offset: number): Promise<MessageSummary[]> {
    calls.list++;
    wake();
    return messages.filter((m) => inView(m, view)).sort(byTop).slice(offset, offset + limit).map(summary);
  },
  /** Cursor opaco como o do backend: `fixada:chave:id` da última linha entregue. */
  async listPage(folder: Role, cursor: string | null, limit: number): Promise<MessagePage> {
    calls.listPage++;
    wake();
    const all = sortedView(folder);
    let from = 0;
    if (cursor) {
      const [p, k, id] = cursor.split(":").map(Number);
      if ([p, k, id].some(Number.isNaN)) throw "cursor inválido";
      // Busca binária: a lista está ordenada por (fixada, chave, id) decrescentes.
      let lo = 0, hi = all.length;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        const m = all[mid];
        const after = Number(!!m.pinned) < p || (Number(!!m.pinned) === p && (topKey(m) < k || (topKey(m) === k && m.id < id)));
        if (after) hi = mid; else lo = mid + 1;
      }
      from = lo;
    }
    const items = all.slice(from, from + limit);
    const last = items[items.length - 1];
    const next = last && from + limit < all.length ? `${Number(!!last.pinned)}:${topKey(last)}:${last.id}` : null;
    return { items: items.map(summary), next };
  },
  async summaries(ids: number[]): Promise<MessageSummary[]> {
    calls.summaries++;
    const want = new Set(ids);
    return messages.filter((m) => want.has(m.id) && !snoozed(m)).sort(byTop).map(summary);
  },
  async search(query: string, view: View): Promise<MessageSummary[]> {
    const terms = norm(query).split(/\s+/).filter(Boolean);
    return messages
      .filter((m) => inView(m, view) && terms.every((t) => norm(`${m.subject} ${m.from_name} ${m.from_addr} ${m.body_text}`).includes(t)))
      .sort((a, b) => b.date - a.date)
      .map(summary);
  },
  async searchFiltered(query: string, view: View): Promise<MessageSummary[]> {
    wake();
    return messages.filter((m) => inView(m, view) && matchFilters(m, query)).sort(byTop).map(summary);
  },
  async searchServer(query: string, view: View): Promise<MessageSummary[]> {
    await wait(700);
    const found = serverOnly.filter((m) => inView(m, view) && matchFilters(m, query));
    for (const m of found) { serverOnly.splice(serverOnly.indexOf(m), 1); messages.push(m); }
    return messages.filter((m) => inView(m, view) && matchFilters(m, query)).sort(byTop).map(summary);
  },
  /** Aplica `settings.rules` à Entrada; devolve quantas mensagens mudaram. */
  async applyRulesNow(): Promise<number> {
    await wait(300);
    const rules = parseRules(settings.rules).filter((r) => r.enabled && r.contains.trim());
    let changed = 0;
    for (const m of messages.filter((x) => x.role === "inbox" && !snoozed(x))) {
      let hit = false;
      for (const r of rules) {
        const hay = r.field === "from" ? `${m.from_name} ${m.from_addr}` : r.field === "to" ? `${m.to_addrs} ${m.cc_addrs}` : m.subject;
        if (!norm(hay).includes(norm(r.contains))) continue;
        hit = true;
        if (r.action === "read") m.seen = true;
        else if (r.action === "star") m.flagged = true;
        else if (r.action === "category") m.category = r.arg;
        else if (r.action === "move" && userFolders.some((f) => f.account === m.account && f.name === r.arg)) { m.role = null; m.box = r.arg; }
        else if (r.action === "archive") m.role = "archive";
      }
      if (hit) changed++;
    }
    return changed;
  },
  async snooze(ids: number[], until: number): Promise<void> {
    await wait(100);
    messages.forEach((m) => { if (ids.includes(m.id)) { m.snoozeUntil = until; m.woke = false; } });
  },
  async unsnooze(ids: number[]): Promise<void> {
    messages.forEach((m) => { if (ids.includes(m.id)) { delete m.snoozeUntil; delete m.woke; } });
  },
  async setPinned(ids: number[], pinned: boolean): Promise<void> {
    await wait(100);
    messages.forEach((m) => { if (ids.includes(m.id)) m.pinned = pinned; });
  },
  async scheduleSend(id: string, sendAt: number, draft: Draft): Promise<void> {
    if (draft.to.includes("erro")) throw "endereço inválido";
    const item: ScheduledItem = { id, account: draft.account, send_at: sendAt, status: "pending", error: "", to: draft.to, subject: draft.subject };
    scheduled = [...scheduled.filter((s) => s.item.id !== id), { item, draft }];
  },
  async listScheduled(): Promise<ScheduledItem[]> {
    return scheduled.map((s) => s.item).sort((a, b) => a.send_at - b.send_at);
  },
  async takeScheduled(id: string): Promise<ScheduledEdit | null> {
    const s = scheduled.find((x) => x.item.id === id);
    if (!s) return null;
    scheduled = scheduled.filter((x) => x !== s);
    return { ...s.draft, reply_to: s.draft.reply_to ?? null, id, send_at: s.item.send_at };
  },
  async thread(id: number): Promise<MessageSummary[]> {
    const m = messages.find((x) => x.id === id);
    if (!m) return [];
    const all = m.thread_id ? messages.filter((x) => x.account === m.account && x.thread_id === m.thread_id && x.role !== "trash" && x.role !== "junk") : [m];
    return all.sort((a, b) => a.date - b.date).map(summary);
  },
  async open(id: number): Promise<MessageDetail | null> {
    const m = messages.find((x) => x.id === id);
    if (!m) return null;
    if (m.noBody) {
      if (offline) throw "Sem conexão: o corpo desta mensagem ainda não foi baixado.";
      m.noBody = false;
    }
    m.seen = true;
    return withCount(m);
  },
  /** Para testar a falha: assunto com "fatura" não move. */
  async move(id: number, role: Role): Promise<void> {
    await wait(400);
    const m = messages.find((x) => x.id === id);
    if (!m) throw "mensagem não encontrada";
    if (m.subject.toLowerCase().includes("fatura")) throw "IMAP: servidor recusou o MOVE";
    m.role = role;
    m.box = undefined;
    queue(1);
  },
  async moveMany(ids: number[], role: Role): Promise<void> {
    await wait(400);
    const found = messages.filter((x) => ids.includes(x.id));
    if (found.some((m) => m.subject.toLowerCase().includes("fatura"))) throw "IMAP: servidor recusou o MOVE";
    found.forEach((m) => { m.role = role; m.box = undefined; });
    queue(found.length);
  },
  async deleteForever(id: number): Promise<void> {
    await wait(400);
    messages = messages.filter((x) => x.id !== id);
    queue(1);
  },
  /** Para testar a falha: anexo com "falha" no nome. */
  async saveAttachment(id: number, index: number, _open: boolean): Promise<string> {
    await wait(500);
    const a = messages.find((x) => x.id === id)?.attachments.find((x) => x.index === index);
    if (!a) throw "anexo não encontrado na mensagem";
    if (offline) throw "Sem conexão: o anexo ainda não foi baixado.";
    if (a.name.includes("falha")) throw "IMAP: conexão recusada";
    return `/home/renan/Downloads/${a.name}`;
  },
  async deleteForeverMany(ids: number[]): Promise<void> {
    await wait(400);
    messages = messages.filter((x) => !ids.includes(x.id));
    queue(ids.length);
  },
  async setSeen(ids: number[], seen: boolean): Promise<void> {
    await wait(150);
    messages.forEach((m) => { if (ids.includes(m.id)) m.seen = seen; });
    queue(ids.length);
  },
  async setFlagged(ids: number[], flagged: boolean): Promise<void> {
    await wait(150);
    messages.forEach((m) => { if (ids.includes(m.id)) m.flagged = flagged; });
    queue(ids.length);
  },
  async unreadCounts(): Promise<UnreadAll> {
    const out: UnreadAll = {};
    for (const m of messages) {
      if (m.seen || !m.role || snoozed(m)) continue;
      const own = (out[m.account] ??= {});
      own[m.role] = (own[m.role] ?? 0) + 1;
    }
    return out;
  },
  async forwardAttachments(id: number): Promise<OutAttachment[]> {
    await wait(200);
    const list = messages.find((x) => x.id === id)?.attachments ?? [];
    return list.map((a) => ({ name: a.name, mime: a.mime, data: btoa(`conteúdo de ${a.name}`.replace(/[^\x00-\xff]/g, "?")) }));
  },
  /** Para testar a falha: qualquer destinatário com "erro". */
  async send(d: Draft): Promise<SendReport> {
    await wait(800);
    if (offline) {
      outbox = [...outbox, { id: Date.now(), account: d.account, to: d.to, subject: d.subject, created: Math.floor(Date.now() / 1000), error: "" }];
      return { saved_to_sent: false, warning: null, queued: true };
    }
    if (d.to.includes("erro")) throw "SMTP: connection refused";
    return { saved_to_sent: true, warning: null, queued: false };
  },
  async queueStatus(): Promise<QueueStatus> {
    return { online: !offline, pending: pendingOps, outbox: outbox.length };
  },
  async flushQueue(): Promise<FlushReport> {
    if (offline) return { applied: 0, discarded: 0, sent: 0, failed: 0, offline: true };
    await wait(400);
    const report = { applied: pendingOps, discarded: 0, sent: outbox.length, failed: 0, offline: false };
    pendingOps = 0;
    outbox = [];
    return report;
  },
  async listOutbox(): Promise<OutboxItem[]> {
    return [...outbox];
  },
  async discardOutbox(id: number): Promise<void> {
    outbox = outbox.filter((m) => m.id !== id);
  },
  async retryOutbox(_id: number): Promise<FlushReport> {
    return this.flushQueue();
  },
};
