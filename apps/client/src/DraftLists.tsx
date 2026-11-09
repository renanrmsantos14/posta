import { PaperPlaneTilt } from "@phosphor-icons/react";
import type { StoredDraft } from "./api";
import { shortDate } from "./dates";
import { whenLabel, type ScheduledItem } from "./productivity";

/** Lista "Agendados": mensagens à espera da hora de envio. Clicar (ou Enter) reabre no Composer. */
export function ScheduledList({ items, onEdit, onCancel }: { items: ScheduledItem[]; onEdit: (s: ScheduledItem) => void; onCancel: (s: ScheduledItem) => void }) {
  if (items.length === 0) return <div className="empty quiet"><p>Nenhuma mensagem agendada</p></div>;
  return (
    <ul className="plain-list" aria-label="Agendados">
      {items.map((s) => (
        <li key={s.id} className="row plain-row draft-row" tabIndex={0} onClick={() => onEdit(s)}
          onKeyDown={(e) => { if (e.key === "Enter" && e.target === e.currentTarget) onEdit(s); }}>
          <PaperPlaneTilt size={13} weight="bold" className="sched-icon" aria-hidden />
          <span className="from">{s.to.trim() || "(sem destinatário)"}</span>
          <time className="date" dateTime={new Date(s.send_at * 1000).toISOString()}>{whenLabel(s.send_at)}</time>
          <span className="subject">{s.subject || "(sem assunto)"}</span>
          <span className="snippet sched-status" data-failed={s.status === "failed" || undefined}>
            {s.status === "failed" ? `Falhou: ${s.error}` : s.status === "sending" ? "Enviando…" : "Aguardando a hora"}
            <button type="button" className="btn ghost small" onClick={(e) => { e.stopPropagation(); onCancel(s); }}>Cancelar envio</button>
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Lista "Rascunhos": rascunhos locais. Clicar (ou Enter) abre no Composer. */
export function DraftList({ items, onOpen }: { items: StoredDraft[]; onOpen: (d: StoredDraft) => void }) {
  if (items.length === 0) return <div className="empty quiet"><p>Nenhum rascunho</p></div>;
  return (
    <ul className="plain-list" aria-label="Rascunhos">
      {items.map((d) => (
        <li key={d.id} className="row plain-row draft-row" tabIndex={0} onClick={() => onOpen(d)}
          onKeyDown={(e) => { if (e.key === "Enter" && e.target === e.currentTarget) onOpen(d); }}>
          <span className="dot" aria-hidden />
          <span className="from">{d.to.trim() || "(sem destinatário)"}</span>
          <time className="date" dateTime={new Date(d.updated * 1000).toISOString()}>{shortDate(d.updated)}</time>
          <span className="subject">{d.subject || "(sem assunto)"}</span>
          <span className="snippet">{d.body.trim().replace(/\s+/g, " ").slice(0, 200)}</span>
        </li>
      ))}
    </ul>
  );
}
