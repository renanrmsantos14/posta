// Escrever e responder. Painel fixo, sem cobrir a mensagem que está sendo respondida.
// Esc ou X perguntam antes de descartar; Ctrl/Cmd+Enter envia.
import { useEffect, useRef, useState } from "react";
import { PaperPlaneTilt, Trash, X } from "@phosphor-icons/react";
import type { Account, Draft } from "./api";

export interface ComposeInit extends Draft {
  title: string;
  /** Voltou de um envio desfeito ou com falha: conta como alterado. */
  restored?: boolean;
}

interface Props {
  init: ComposeInit;
  accounts: Account[];
  onSend: (draft: ComposeInit) => void;
  onDiscard: () => void;
}

const EXIT_MS = 150;
const mac = /Mac|iPhone|iPad/.test(navigator.platform);

/** Cada destinatário precisa ter algo@algo. */
function invalidRecipient(to: string): string | null {
  const list = to.split(/[,;]/).map((s) => s.trim()).filter(Boolean);
  if (!list.length) return "Informe para quem vai a mensagem.";
  const bad = list.find((s) => !/[^\s<@]+@[^\s>@]+\.[^\s>@]+/.test(s));
  return bad ? `Endereço inválido: ${bad}` : null;
}

export function Composer({ init, accounts, onSend, onDiscard }: Props) {
  const [draft, setDraft] = useState(init);
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const toRef = useRef<HTMLInputElement>(null);
  const bodyRef = useRef<HTMLTextAreaElement>(null);

  const dirty = !!init.restored || draft.to !== init.to || draft.subject !== init.subject || draft.body !== init.body;

  // Resposta: cursor no topo do corpo, acima da citação. Nova: começa pelo destinatário.
  useEffect(() => {
    if (init.reply_to !== null && init.to) {
      bodyRef.current?.focus();
      bodyRef.current?.setSelectionRange(0, 0);
      bodyRef.current?.scrollTo(0, 0);
    } else {
      toRef.current?.focus();
    }
  }, [init]);

  function leave(then: () => void) {
    setLeaving(true);
    setTimeout(then, EXIT_MS);
  }

  function requestClose() {
    if (dirty) setConfirming(true);
    else leave(onDiscard);
  }

  function send() {
    const problem = invalidRecipient(draft.to);
    if (problem) {
      setError(problem);
      setErrorKey((k) => k + 1);
      toRef.current?.focus();
      return;
    }
    leave(() => onSend(draft));
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        if (confirming) setConfirming(false);
        else requestClose();
      } else if (e.key === "Enter" && (mac ? e.metaKey : e.ctrlKey)) {
        e.preventDefault();
        e.stopPropagation();
        send();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  const set = <K extends keyof ComposeInit>(k: K, v: ComposeInit[K]) => {
    setDraft((d) => ({ ...d, [k]: v }));
    if (k === "to") setError("");
  };

  return (
    <section className="composer" role="dialog" aria-labelledby="composer-title" data-leaving={leaving || undefined}>
      <header className="composer-head">
        <h2 id="composer-title">{draft.title}</h2>
        <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose}>
          <X size={16} weight="bold" />
        </button>
      </header>

      <div className="composer-fields">
        {accounts.length > 1 ? (
          <label className="line">
            <span>De</span>
            <select value={draft.account} onChange={(e) => set("account", e.target.value)}>
              {accounts.map((a) => <option key={a.id} value={a.id}>{a.username}</option>)}
            </select>
          </label>
        ) : (
          <div className="line from-line">
            <span>De</span>
            <span className="from-value">{accounts.find((a) => a.id === draft.account)?.username}</span>
          </div>
        )}
        <label className="line" data-invalid={error ? true : undefined}>
          <span>Para</span>
          <input ref={toRef} type="text" inputMode="email" autoComplete="email" value={draft.to}
            onChange={(e) => set("to", e.target.value)} placeholder="nome@exemplo.com" aria-invalid={!!error} />
        </label>
        {error && <p key={errorKey} className="form-error composer-error" role="alert">{error}</p>}
        <label className="line">
          <span>Assunto</span>
          <input type="text" value={draft.subject} onChange={(e) => set("subject", e.target.value)} />
        </label>
      </div>

      <textarea ref={bodyRef} className="composer-body" value={draft.body} aria-label="Mensagem"
        onChange={(e) => set("body", e.target.value)} />

      <footer className="composer-foot">
        <button type="button" className="btn primary" onClick={send}>
          <PaperPlaneTilt size={15} weight="fill" /> Enviar
        </button>
        <span className="hint-keys"><kbd>{mac ? "⌘" : "Ctrl"}</kbd><kbd>↵</kbd></span>
        <button type="button" className="icon-btn push" aria-label="Descartar rascunho" onClick={() => leave(onDiscard)}>
          <Trash size={16} weight="bold" />
        </button>
      </footer>

      {confirming && (
        <div className="confirm" role="alertdialog" aria-labelledby="discard-title">
          <p id="discard-title">Descartar rascunho?</p>
          <div className="confirm-actions">
            <button type="button" className="btn" onClick={() => setConfirming(false)} autoFocus>Continuar editando</button>
            <button type="button" className="btn danger" onClick={() => leave(onDiscard)}>Descartar</button>
          </div>
        </div>
      )}
    </section>
  );
}
