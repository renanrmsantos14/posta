// Ações do leitor no celular (no desktop quem cuida disso é a faixa de comandos).
// Barra presa no rodapé, na zona do polegar: as ações frequentes ficam à vista e o resto vai para o menu "Mais".
import { useRef, useState } from "react";
import { Archive, ArrowBendDoubleUpLeft, ArrowBendUpLeft, ArrowBendUpRight, Clock, DotsThree, EnvelopeSimple, PushPin, Star, Tray, Trash } from "@phosphor-icons/react";
import { CategoryItems } from "./Categories";
import { useDismiss, WhenMenu } from "./WhenMenu";
import type { Category, MessageSummary } from "./api";

export interface MobileActionsProps {
  current: MessageSummary | null;
  categories: Category[];
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
  const [more, setMore] = useState(false);
  const moreRef = useRef<HTMLSpanElement>(null);
  useDismiss(more, () => setMore(false), moreRef);
  /** Fecha o menu antes de agir, para a ação não ficar escondida atrás dele. */
  const pick = (run: () => void) => () => { setMore(false); run(); };

  return (
  <div className="actions mobile-actions" role="toolbar" aria-label="Ações da mensagem">
    <button className="btn reply-btn" onClick={p.onReply}>
      <ArrowBendUpLeft size={16} weight="bold" /> <span>Responder</span>
    </button>
    {p.canArchive && (
      <button className="btn icon-only" onClick={p.onArchive}
        aria-label={p.toInbox ? "Mover para a entrada" : "Arquivar"} title={p.toInbox ? "Mover para a entrada" : "Arquivar"}>
        {p.toInbox ? <Tray size={18} weight="bold" /> : <Archive size={18} weight="bold" />}
      </button>
    )}
    <button className="btn icon-only" onClick={p.onTrash} disabled={!p.canTrash}
      aria-label={p.inTrash ? "Excluir de vez" : "Excluir"} title={!p.canTrash ? "Esta conta não tem Lixeira" : "Excluir"}>
      <Trash size={18} weight="bold" />
    </button>
    {current && (
      <>
        <button className="btn icon-only star-btn" data-on={current.flagged || undefined} data-pop={p.popped || undefined}
          onClick={p.onStar} aria-pressed={current.flagged}
          aria-label={current.flagged ? "Tirar a estrela" : "Marcar com estrela"} title="Estrela">
          <Star size={18} weight={current.flagged ? "fill" : "bold"} />
        </button>
        <WhenMenu className="btn icon-only" label="Adiar" up onPick={(at) => p.onSnooze(at)}><Clock size={18} weight="bold" /></WhenMenu>
      </>
    )}
    <span className="cat-picker" ref={moreRef}>
      <button type="button" className="btn icon-only" aria-label="Mais ações" title="Mais ações" aria-haspopup="menu" aria-expanded={more}
        onClick={() => setMore((o) => !o)}>
        <DotsThree size={20} weight="bold" />
      </button>
      {more && (
        <div className="cat-menu more-menu" role="menu" aria-label="Mais ações">
          <button type="button" role="menuitem" onClick={pick(p.onReplyAll)}><ArrowBendDoubleUpLeft size={16} weight="bold" /> <span>Responder a todos</span></button>
          <button type="button" role="menuitem" onClick={pick(p.onForward)}><ArrowBendUpRight size={16} weight="bold" /> <span>Encaminhar</span></button>
          {current && (
            <>
              <button type="button" role="menuitem" onClick={pick(p.onUnread)}><EnvelopeSimple size={16} weight="bold" /> <span>Marcar como não lida</span></button>
              <button type="button" role="menuitem" onClick={pick(p.onPin)}>
                <PushPin size={16} weight={current.pinned ? "fill" : "bold"} /> <span>{current.pinned ? "Desafixar" : "Fixar no topo"}</span>
              </button>
              <div className="more-group" role="group" aria-label="Categoria">
                <p className="more-label">Categoria</p>
                <CategoryItems categories={p.categories} current={current.category ?? null} onPick={(id) => { setMore(false); p.onCategory(id); }} />
              </div>
            </>
          )}
        </div>
      )}
    </span>
  </div>
  );
}
