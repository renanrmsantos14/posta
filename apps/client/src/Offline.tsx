import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowClockwise, Trash, WifiSlash } from "@phosphor-icons/react";
import { api, isTauri, type OutboxItem, type QueueStatus } from "./api";
import { toast } from "./toast";

const POLL_MS = 15_000;
const when = new Intl.DateTimeFormat("pt-BR", { timeZone: "America/Sao_Paulo", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

/**
 * Estado da rede e da fila offline. Consulta o núcleo a cada 15 s e quando o sistema avisa (`online`/`offline`,
 * evento `queue-changed`). Ao voltar a rede com algo na fila, pede o reenvio e chama `onFlushed` (recarregar listas).
 */
export function useQueue(onFlushed: () => void) {
  const [status, setStatus] = useState<QueueStatus>({ online: true, pending: 0, outbox: 0 });
  const was = useRef(true);
  const flushed = useRef(onFlushed);
  flushed.current = onFlushed;

  const refresh = useCallback(async () => {
    let s: QueueStatus;
    try { s = await api.queueStatus(); } catch { return; }
    setStatus(s);
    const back = s.online && (!was.current || s.pending > 0 || s.outbox > 0);
    was.current = s.online;
    if (!back) return;
    try {
      const r = await api.flushQueue();
      if (r.sent) toast(r.sent === 1 ? "Mensagem da Caixa de saída enviada" : `${r.sent} mensagens da Caixa de saída enviadas`, "success");
      if (r.failed) toast("O servidor recusou uma mensagem da Caixa de saída. Veja o motivo lá.", "error");
      if (r.applied + r.discarded + r.sent + r.failed > 0) {
        setStatus(await api.queueStatus());
        flushed.current();
      }
    } catch { /* a próxima rodada tenta de novo */ }
  }, []);

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, POLL_MS);
    const now = () => { refresh(); };
    window.addEventListener("online", now);
    window.addEventListener("offline", now);
    let off: (() => void) | undefined;
    let alive = true;
    if (isTauri) {
      import("@tauri-apps/api/event")
        .then(({ listen }) => listen("queue-changed", () => { refresh(); flushed.current(); }))
        .then((un) => (alive ? (off = un) : un()));
    }
    return () => {
      clearInterval(t);
      window.removeEventListener("online", now);
      window.removeEventListener("offline", now);
      alive = false;
      off?.();
    };
  }, [refresh]);

  return { status, refresh };
}

/** Indicador discreto na barra de ferramentas: só aparece sem conexão. */
export function OfflineBadge({ status }: { status: QueueStatus }) {
  if (status.online) return null;
  const waiting = status.pending + status.outbox;
  return (
    <span className="offline-badge" role="status" title="Sem conexão: você lê o que já está salvo e as ações esperam a rede voltar">
      <WifiSlash size={13} weight="bold" aria-hidden />
      Offline{waiting > 0 && ` · ${waiting} ${waiting === 1 ? "pendente" : "pendentes"}`}
    </span>
  );
}

/** Caixa de saída: o que foi escrito sem rede (ou que o servidor recusou) e ainda não saiu. */
export function OutboxPane({ items, accounts, online, onChange }: {
  items: OutboxItem[]; accounts: string[]; online: boolean; onChange: () => void;
}) {
  async function discard(id: number) {
    await api.discardOutbox(id).catch((e) => toast(`${e}`, "error"));
    onChange();
  }
  async function retry(id: number) {
    try {
      await api.retryOutbox(id);
    } catch (e) {
      toast(`${e}`, "error");
    }
    onChange();
  }
  if (items.length === 0) return <div className="empty quiet"><p>Nada na Caixa de saída</p></div>;
  return (
    <ul className="list" role="list" aria-label="Caixa de saída">
      {items.map((m) => (
        <li key={m.id} className="row draft-row outbox-row" data-failed={m.error ? "" : undefined}>
          <span className="dot" aria-hidden />
          <span className="from">{m.to.trim() || "(sem destinatário)"}</span>
          <time className="date" dateTime={new Date(m.created * 1000).toISOString()}>{when.format(new Date(m.created * 1000))}</time>
          <span className="subject">{m.subject || "(sem assunto)"}</span>
          <span className="snippet outbox-state">
            {m.error ? `Não saiu: ${m.error}` : online ? "Enviando…" : "Aguardando conexão"}
            {accounts.length > 1 && ` · ${m.account}`}
          </span>
          <span className="outbox-actions">
            {m.error && (
              <button className="icon-btn" onClick={() => retry(m.id)} aria-label="Tentar de novo" title="Tentar de novo">
                <ArrowClockwise size={15} weight="bold" />
              </button>
            )}
            <button className="icon-btn" onClick={() => discard(m.id)} aria-label="Descartar" title="Descartar">
              <Trash size={15} weight="bold" />
            </button>
          </span>
        </li>
      ))}
    </ul>
  );
}
