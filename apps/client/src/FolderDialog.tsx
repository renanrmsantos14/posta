// Criar pasta (ou subpasta) e renomear. Fecha por X, Esc ou clique fora; pergunta antes de descartar o que foi digitado.
import { useEffect, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";
import type { Account, UserFolder } from "./api";
import { folderTree, type FolderNode } from "./folders";

export type FolderDialogState =
  | { mode: "create"; account?: string; parent?: string }
  | { mode: "rename"; node: FolderNode };

interface Props {
  state: FolderDialogState;
  accounts: Account[];
  folders: UserFolder[];
  /** Resolve quando o servidor aceitou; rejeita com a mensagem de erro. */
  onSubmit: (account: string, parent: string | null, name: string) => Promise<void>;
  onClose: () => void;
}

const EXIT_MS = 150;

export function FolderDialog({ state, accounts, folders, onSubmit, onClose }: Props) {
  const rename = state.mode === "rename";
  const initial = rename ? state.node.leaf : "";
  const [account, setAccount] = useState(rename ? state.node.account : state.account ?? accounts[0]?.id ?? "");
  const [parent, setParent] = useState(rename ? "" : state.parent ?? "");
  const [name, setName] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const dirty = name.trim() !== initial;
  const parents = folderTree(folders).filter((f) => f.account === account);

  useEffect(() => { input.current?.focus(); input.current?.select(); }, []);

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }

  function requestClose() {
    if (busy) return;
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

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const leaf = name.trim();
    if (!leaf || busy) return;
    if (rename && leaf === initial) return close();
    setError("");
    setBusy(true);
    try {
      await onSubmit(account, parent || null, leaf);
      close();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  }

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="folder-title" onSubmit={submit}>
        <header className="dialog-head">
          <h2 id="folder-title">{rename ? "Renomear pasta" : parent ? "Nova subpasta" : "Nova pasta"}</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={busy}>
            <X size={16} weight="bold" />
          </button>
        </header>

        {!rename && accounts.length > 1 && (
          <label className="field">
            <span>Conta</span>
            <select value={account} onChange={(e) => { setAccount(e.target.value); setParent(""); }}>
              {accounts.map((a) => <option key={a.id} value={a.id}>{a.username}</option>)}
            </select>
          </label>
        )}
        {!rename && (
          <label className="field">
            <span>Dentro de</span>
            <select value={parent} onChange={(e) => setParent(e.target.value)}>
              <option value="">Nenhuma (pasta principal)</option>
              {parents.map((f) => <option key={f.name} value={f.name}>{" ".repeat(f.depth)}{f.label}</option>)}
            </select>
          </label>
        )}
        <label className="field">
          <span>Nome</span>
          <input ref={input} required maxLength={80} value={name} onChange={(e) => setName(e.target.value)} placeholder="Clientes" />
        </label>

        {error && <p className="form-error" role="alert">{error}</p>}

        <button className="btn primary wide" disabled={busy || !name.trim()}>
          {busy ? <><span className="spinner" aria-hidden />{rename ? "Renomeando" : "Criando"}</> : rename ? "Renomear" : "Criar pasta"}
        </button>

        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="folder-confirm">
            <p id="folder-confirm">Descartar alterações?</p>
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
