// Painel de leitura: cabeçalho, destinatários (com cartão de contato), anexos, corpo (HtmlBody) e a conversa dobrável.
import type { ReactNode } from "react";
import { ArrowLeft, ArrowsIn, ArrowsOut, CaretUp, Paperclip, ShieldWarning, WarningCircle, WifiSlash } from "@phosphor-icons/react";
import { HtmlBody, ZoomControls, hasRemoteImages } from "./HtmlBody";
import { PageLayout } from "./layout/primitives";
import { parseContact } from "./ContactCard";
import { addrOf } from "./aliases";
import { fmtLong, shortDate } from "./dates";
import { fmtSize } from "./format";
import { suspiciousSender } from "./phishing";
import type { Attachment, MessageDetail, MessageSummary } from "./api";

export const splitAddrs = (v: string) => v.split(/,(?=(?:[^"]*"[^"]*")*[^"]*$)/).map((x) => x.trim()).filter(Boolean);

export interface ReaderPaneProps {
  /** Mensagem aberta (já filtrada pela seleção); null mostra o painel vazio. */
  detail: MessageDetail | null;
  /** Motivo de a mensagem selecionada não ter aberto; mostra um estado vazio com "Tentar de novo". */
  error: string | null;
  onRetry: () => void;
  selectedId: number | null;
  threaded: boolean;
  thread: MessageSummary[];
  bodies: Record<number, MessageDetail>;
  expanded: Set<number>;
  full: boolean;
  viaKeyboard: boolean;
  folderLabel: string;
  hasMessages: boolean;
  imagesOk: Set<number>;
  /** Remetentes com imagens remotas sempre liberadas (setting `images:trusted`). */
  imagesFrom: Set<string>;
  savingAtt: string | null;
  /** Barra de ações do celular (a faixa de comandos cobre o desktop). */
  mobileActions: ReactNode;
  /** Diálogos que cobrem o leitor (confirmação de exclusão). */
  children?: ReactNode;
  onFull: (on: boolean) => void;
  onBack: () => void;
  onContact: (e: React.MouseEvent<HTMLElement>, addr: string) => void;
  onReaderContext: (x: number, y: number, selection: string) => void;
  onToggleThread: (id: number) => void;
  onShowImages: (id: number) => void;
  onTrustSender: (addr: string) => void;
  /** "Relatar phishing" do aviso de remetente suspeito: pede confirmação e move para o Spam. */
  onReportPhishing: (d: MessageDetail) => void;
  onSaveAttachment: (id: number, a: Attachment) => void;
  /** Botão pressionado no anexo: arrastar para fora da janela entrega o arquivo ao sistema. */
  onDragAttachment: (e: React.PointerEvent, id: number, a: Attachment) => void;
}

function Recipients({ label, v, onContact }: { label: string; v: string; onContact: ReaderPaneProps["onContact"] }) {
  const list = splitAddrs(v);
  if (!list.length) return null;
  return (
    <div className="meta-to">
      <span className="meta-to-label">{label}</span>
      {list.slice(0, 6).map((a) => {
        const c = parseContact(a);
        return <button key={a} type="button" className="contact-chip" title={c.addr} onClick={(e) => onContact(e, a)}>{c.name || c.addr}</button>;
      })}
      {list.length > 6 && <span className="meta-more">+{list.length - 6}</span>}
    </div>
  );
}

function MessageView({ d, fold, p }: { d: MessageDetail; fold: boolean; p: ReaderPaneProps }) {
  const fromAddr = addrOf(d.from_addr);
  const remote = p.imagesOk.has(d.id) || p.imagesFrom.has(fromAddr);
  const suspicious = d.role === "junk" ? null : suspiciousSender(d.from_name, d.from_addr, d.reply_addr);
  const sender = d.from_name ? `${d.from_name} <${d.from_addr}>` : d.from_addr;
  return (
    <>
      <div className="meta">
        <span className="avatar large" aria-hidden>{(d.from_name || d.from_addr)[0]?.toUpperCase()}</span>
        <div className="meta-id">
          <div className="meta-from"><button type="button" className="contact-link" onClick={(e) => p.onContact(e, sender)}>{d.from_name || d.from_addr}</button></div>
          {d.from_name && <div className="meta-addr">{d.from_addr}</div>}
        </div>
        <time className="meta-date">{fmtLong.format(new Date(d.date * 1000))}</time>
        {d.body_html && !fold && <ZoomControls />}
        {fold ? (
          <button type="button" className="icon-btn expand-btn" onClick={() => p.onToggleThread(d.id)}
            aria-label="Recolher mensagem" title="Recolher">
            <CaretUp size={15} weight="bold" />
          </button>
        ) : (
          <button type="button" className="icon-btn expand-btn" onClick={() => p.onFull(!p.full)}
            aria-label={p.full ? "Sair da tela cheia" : "Expandir mensagem"} title={p.full ? "Voltar (Esc)" : "Expandir (clique duplo na lista)"}>
            {p.full ? <ArrowsIn size={15} weight="bold" /> : <ArrowsOut size={15} weight="bold" />}
          </button>
        )}
      </div>
      <Recipients label="Para" v={d.to_addrs} onContact={p.onContact} />
      <Recipients label="Cc" v={d.cc_addrs} onContact={p.onContact} />
      {suspicious && (
        <div className="img-banner" role="status">
          <span><ShieldWarning size={15} weight="bold" aria-hidden /> {suspicious} Confira antes de clicar em links ou responder.</span>
          <span className="img-banner-actions">
            <button type="button" className="btn" onClick={() => p.onReportPhishing(d)}>Relatar phishing</button>
          </span>
        </div>
      )}
      {!p.threaded && d.id === p.selectedId && p.mobileActions}
      {d.attachments.length > 0 && (
        <ul className="attachments" aria-label="Anexos">
          {d.attachments.map((a) => (
            <li key={a.index}>
              <button type="button" className="chip attachment" disabled={p.savingAtt === `${d.id}:${a.index}`}
                title={`Salvar e abrir ${a.name}`} onClick={() => p.onSaveAttachment(d.id, a)}
                onPointerDown={(e) => p.onDragAttachment(e, d.id, a)}>
                <Paperclip size={14} weight="bold" aria-hidden />
                <span className="chip-name">{a.name}</span>
                <span className="chip-size">{fmtSize(a.size)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {d.body_html ? (
        <>
          {!remote && hasRemoteImages(d.body_html) && (
            <div className="img-banner" role="status">
              <span>Imagens remotas bloqueadas para proteger sua privacidade.</span>
              <span className="img-banner-actions">
                <button type="button" className="btn" onClick={() => p.onShowImages(d.id)}>
                  Mostrar imagens
                </button>
                {fromAddr && (
                  <button type="button" className="btn" onClick={() => p.onTrustSender(fromAddr)}>
                    Sempre mostrar de {fromAddr}
                  </button>
                )}
              </span>
            </div>
          )}
          <HtmlBody html={d.body_html} remote={remote} onContext={p.onReaderContext} />
        </>
      ) : (
        <div className="body">{d.body_text}</div>
      )}
    </>
  );
}

export function ReaderPane(p: ReaderPaneProps) {
  return (
    <PageLayout area="reader" as="main" aria-label="Leitura">
        {p.detail ? (
          <article className="message" data-instant={p.viaKeyboard || undefined}
            onContextMenu={(e) => {
              if ((e.target as Element).closest("a, button, input, textarea")) return;
              e.preventDefault();
              p.onReaderContext(e.clientX, e.clientY, window.getSelection()?.toString() ?? "");
            }}>
            <button className="btn ghost small back" onClick={() => p.onBack()}>
              <ArrowLeft size={14} weight="bold" /> {p.full ? "Voltar" : p.folderLabel}
            </button>
            <h1>{p.detail.subject || "(sem assunto)"}</h1>
            {p.threaded && p.mobileActions}
            {p.threaded ? (
              <div className="thread">
                {p.thread.map((t) => {
                  const d = t.id === p.detail!.id ? p.detail : p.bodies[t.id];
                  if (d && (t.id === p.detail!.id || p.expanded.has(t.id))) {
                    return <section key={t.id} className="thread-msg" data-current={t.id === p.detail!.id || undefined}><MessageView d={d} fold={t.id !== p.detail!.id} p={p} /></section>;
                  }
                  return (
                    <button key={t.id} type="button" className="thread-fold" data-unread={!t.seen || undefined} aria-expanded="false"
                      onClick={() => p.onToggleThread(t.id)}>
                      <span className="avatar" aria-hidden>{(t.from_name || t.from_addr)[0]?.toUpperCase()}</span>
                      <span className="fold-from">{t.from_name || t.from_addr}</span>
                      <span className="fold-snip">{t.snippet}</span>
                      <time className="fold-date">{shortDate(t.date)}</time>
                    </button>
                  );
                })}
              </div>
            ) : <MessageView d={p.detail} fold={false} p={p} />}
          </article>
        ) : p.error ? (
          <div className="reader-empty reader-error" role="alert">
            <button className="btn ghost small back" onClick={() => p.onBack()}>
              <ArrowLeft size={14} weight="bold" /> {p.full ? "Voltar" : p.folderLabel}
            </button>
            {p.error.startsWith("Sem conexão") ? <WifiSlash size={32} weight="duotone" aria-hidden /> : <WarningCircle size={32} weight="duotone" aria-hidden />}
            <h2>Não foi possível abrir a mensagem</h2>
            <p>{p.error}</p>
            <button type="button" className="btn" onClick={() => p.onRetry()} autoFocus>Tentar de novo</button>
          </div>
        ) : (
          <div className="reader-empty">
            <p>{p.hasMessages ? "Selecione uma mensagem" : ""}</p>
          </div>
        )}
      {p.children}
    </PageLayout>
  );
}
