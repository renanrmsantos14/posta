// "Escolher data e hora…" do Adiar (menus da barra e do clique direito). Fecha por X, Esc ou clique fora; mudou a hora, pergunta antes de descartar.
import { useEffect, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";
import { fromLocalInput, isPast, toLocalInput, whenLabel } from "./productivity";

interface Props {
  title: string;
  /** Texto do botão de confirmar. */
  confirm: string;
  onPick: (unix: number) => void;
  onClose: () => void;
}

const EXIT_MS = 150;

export function WhenDialog({ title, confirm, onPick, onClose }: Props) {
  const initial = useRef(toLocalInput(Math.floor(Date.now() / 1000) + 3600)).current;
  const [value, setValue] = useState(initial);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const at = fromLocalInput(value);
  const past = Number.isNaN(at) || isPast(at);
  const dirty = value !== initial;

  useEffect(() => input.current?.focus(), []);

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }
  const requestClose = () => (dirty ? setConfirming(true) : close());
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      if (confirming) setConfirming(false);
      else requestClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="when-title"
        onSubmit={(e) => { e.preventDefault(); if (!past) { onPick(at); close(); } }}>
        <header className="dialog-head">
          <h2 id="when-title">{title}</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose}><X size={16} weight="bold" /></button>
        </header>
        <label className="field">
          <span>Data e hora (Brasília)</span>
          <input ref={input} type="datetime-local" required value={value} min={toLocalInput(Math.floor(Date.now() / 1000))} onChange={(e) => setValue(e.target.value)} aria-invalid={(value !== "" && past) || undefined} />
        </label>
        {value !== "" && past ? <p className="form-error" role="alert">Esse horário já passou.</p> : <p className="when-hint">{Number.isNaN(at) ? "" : whenLabel(at)}</p>}
        <button className="btn primary wide" disabled={past}>{confirm}</button>
        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="when-confirm">
            <p id="when-confirm">Descartar alterações?</p>
            <div className="confirm-actions">
              <button type="button" className="btn" onClick={() => setConfirming(false)} autoFocus>Continuar editando</button>
              <button type="button" className="btn danger" onClick={close}>Descartar</button>
            </div>
          </div>
        )}
      </form>
    </div>
  );
}
