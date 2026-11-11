// Salvar pesquisa (pede o nome) e renomear. Fecha por X, Esc ou clique fora; pergunta antes de descartar o que foi digitado.
import { useEffect, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";

export type SearchDialogState = { mode: "create"; query: string; name: string } | { mode: "rename"; id: string; query: string; name: string };

interface Props {
  state: SearchDialogState;
  onSubmit: (name: string) => void;
  onClose: () => void;
}

const EXIT_MS = 150;

export function SearchDialog({ state, onSubmit, onClose }: Props) {
  const rename = state.mode === "rename";
  const [name, setName] = useState(state.name);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const dirty = rename && name.trim() !== state.name;

  useEffect(() => { input.current?.focus(); input.current?.select(); }, []);

  function close() {
    if (leaving) return;
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }
  function requestClose() {
    if (dirty) setConfirming(true);
    else close();
  }

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

  function submit(e: React.FormEvent) {
    e.preventDefault();
    const n = name.trim();
    if (!n || leaving) return;
    onSubmit(n);
    close();
  }

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="search-title" onSubmit={submit}>
        <header className="dialog-head">
          <h2 id="search-title">{rename ? "Renomear pesquisa" : "Salvar pesquisa"}</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose}>
            <X size={16} weight="bold" />
          </button>
        </header>
        <label className="field">
          <span>Nome</span>
          <input ref={input} required maxLength={60} value={name} onChange={(e) => setName(e.target.value)} placeholder="Anexos do João" />
        </label>
        <p className="muted" style={{ margin: 0, fontSize: 12 }}>Busca: <code>{state.query}</code></p>
        <button className="btn primary wide" disabled={!name.trim()}>{rename ? "Renomear" : "Salvar pesquisa"}</button>
        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="search-confirm">
            <p id="search-confirm">Descartar alterações?</p>
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
