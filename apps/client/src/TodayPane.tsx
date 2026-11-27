// "Hoje": resumo das últimas 24 h em quatro grupos, calculado no núcleo a partir do banco local (sem rede, sem IA).
// Clicar numa mensagem abre o leitor; nada aqui muda a caixa de entrada.
import { useState } from "react";
import { CaretDown, CaretRight, SunHorizon } from "@phosphor-icons/react";
import type { MessageSummary } from "./api";
import { shortDate } from "./dates";
import { DIGEST_GROUPS, digestTotal, type Digest, type DigestKey } from "./digest";

interface Props {
  digest: Digest | null;
  error: string | null;
  selectedId: number | null;
  onOpen: (m: MessageSummary) => void;
  onRetry: () => void;
}

export function TodayPane({ digest, error, selectedId, onOpen, onRetry }: Props) {
  // Grupos fechados nesta visita; abrem de novo ao voltar para a tela.
  const [closed, setClosed] = useState<Set<DigestKey>>(new Set());
  if (error) {
    return (
      <div className="empty quiet" role="alert">
        <p>Não consegui montar o resumo: {error}</p>
        <button className="btn small" onClick={onRetry}>Tentar de novo</button>
      </div>
    );
  }
  if (!digest) return <div className="empty quiet"><p>Carregando…</p></div>;
  if (digestTotal(digest) === 0) {
    return (
      <div className="empty quiet">
        <SunHorizon size={28} weight="duotone" />
        <p>Nada chegou nas últimas 24 horas.</p>
      </div>
    );
  }
  const toggle = (k: DigestKey) => setClosed((c) => { const n = new Set(c); if (!n.delete(k)) n.add(k); return n; });
  return (
    <div className="today" role="region" aria-label="Resumo das últimas 24 horas">
      <p className="today-meta">Últimas 24 horas · {digestTotal(digest)} {digestTotal(digest) === 1 ? "mensagem" : "mensagens"}</p>
      {DIGEST_GROUPS.map(({ key, label, hint }) => {
        const items = digest[key];
        const open = !closed.has(key);
        const unread = items.filter((m) => !m.seen).length;
        return (
          <section key={key} className="today-group" data-group={key} data-empty={items.length === 0 || undefined}>
            <button type="button" className="today-head" aria-expanded={open} aria-controls={`today-${key}`} onClick={() => toggle(key)} title={hint}>
              {open ? <CaretDown size={12} weight="bold" aria-hidden /> : <CaretRight size={12} weight="bold" aria-hidden />}
              <span className="today-title">{label}</span>
              <span className="today-count">{items.length}{unread > 0 && <> · {unread} não {unread === 1 ? "lida" : "lidas"}</>}</span>
            </button>
            {open && (
              <ul id={`today-${key}`} className="plain-list today-list" aria-label={label}>
                {items.length === 0 && <li className="today-none">Nada por aqui.</li>}
                {items.map((m) => (
                  <li key={m.id} className="row plain-row draft-row" tabIndex={0} aria-selected={m.id === selectedId} data-unread={!m.seen || undefined}
                    onClick={() => onOpen(m)} onKeyDown={(e) => { if (e.key === "Enter" && e.target === e.currentTarget) onOpen(m); }}>
                    <span className="dot" aria-hidden />
                    <span className="from from-name">{m.from_name || m.from_addr}</span>
                    <time className="date" dateTime={new Date(m.date * 1000).toISOString()}>{shortDate(m.date)}</time>
                    <span className="subject">{m.subject || "(sem assunto)"}</span>
                    <span className="snippet">{m.snippet}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}
