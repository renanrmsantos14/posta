// Ações do leitor no celular (no desktop quem cuida disso é a faixa de comandos).
import { Archive, ArrowBendDoubleUpLeft, ArrowBendUpLeft, ArrowBendUpRight, Clock, EnvelopeSimple, PushPin, Star, Tray, Trash } from "@phosphor-icons/react";
import { CategoryPicker } from "./Categories";
import { WhenMenu } from "./WhenMenu";
import type { MessageSummary } from "./api";

type Categories = Parameters<typeof CategoryPicker>[0]["categories"];

export interface MobileActionsProps {
  current: MessageSummary | null;
  categories: Categories;
  canArchive: boolean;
  canTrash: boolean;
  /** Arquivar devolve à entrada (a mensagem está na Lixeira ou no Spam). */
  toInbox: boolean;
  inTrash: boolean;
  popped: boolean;
  onReply: () => void; onReplyAll: () => void; onForward: () => void;
  onArchive: () => void; onTrash: () => void; onStar: () => void; onUnread: () => void; onPin: () => void;
  onCategory: (id: string | null) => void; onSnooze: (at: number) => void;
}

export function MobileActions(p: MobileActionsProps) {
  const { current } = p;
  return (
  <div className="actions mobile-actions">
    <button className="btn" onClick={p.onReply}>
      <ArrowBendUpLeft size={15} weight="bold" /> Responder
    </button>
    <button className="btn icon-only" onClick={p.onReplyAll} aria-label="Responder a todos" title="Responder a todos">
      <ArrowBendDoubleUpLeft size={15} weight="bold" />
    </button>
    <button className="btn icon-only" onClick={p.onForward} aria-label="Encaminhar" title="Encaminhar">
      <ArrowBendUpRight size={15} weight="bold" />
    </button>
    {p.canArchive && (
      <button className="btn icon-only" onClick={p.onArchive}
        aria-label={p.toInbox ? "Mover para a entrada" : "Arquivar"} title={p.toInbox ? "Mover para a entrada" : "Arquivar"}>
        {p.toInbox ? <Tray size={15} weight="bold" /> : <Archive size={15} weight="bold" />}
      </button>
    )}
    <button className="btn icon-only" onClick={p.onTrash} disabled={!p.canTrash}
      aria-label={p.inTrash ? "Excluir de vez" : "Excluir"} title={!p.canTrash ? "Esta conta não tem Lixeira" : "Excluir"}>
      <Trash size={15} weight="bold" />
    </button>
    {current && (
      <>
        <button className="btn icon-only star-btn" data-on={current.flagged || undefined} data-pop={p.popped || undefined}
          onClick={p.onStar} aria-pressed={current.flagged}
          aria-label={current.flagged ? "Tirar a estrela" : "Marcar com estrela"} title="Estrela">
          <Star size={15} weight={current.flagged ? "fill" : "bold"} />
        </button>
        <button className="btn icon-only" onClick={p.onUnread}
          aria-label="Marcar como não lida" title="Marcar como não lida">
          <EnvelopeSimple size={15} weight="bold" />
        </button>
        <CategoryPicker className="btn icon-only" categories={p.categories} current={current.category ?? null} onPick={(id) => p.onCategory(id)} />
        <WhenMenu className="btn icon-only" label="Adiar" onPick={(at) => p.onSnooze(at)}><Clock size={15} weight="bold" /></WhenMenu>
        <button className="btn icon-only star-btn" data-on={current.pinned || undefined} onClick={p.onPin}
          aria-pressed={!!current.pinned} aria-label={current.pinned ? "Desafixar" : "Fixar no topo"} title="Fixar no topo">
          <PushPin size={15} weight={current.pinned ? "fill" : "bold"} />
        </button>
      </>
    )}
  </div>
  );
}
