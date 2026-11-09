// Escrever e responder. Painel fixo, sem cobrir a mensagem que está sendo respondida.
// Salva sozinho como rascunho (debounce de 1,5 s); Esc e X guardam o rascunho e fecham, a lixeira descarta
// (com confirmação se há texto). Ctrl/Cmd+Enter envia.
import { useEffect, useRef, useState } from "react";
import { PaperPlaneTilt, Trash, X } from "@phosphor-icons/react";
import { api, type Account, type Draft } from "./api";
import { toast } from "./toast";

export interface ComposeInit extends Draft {
  title: string;
  /** Voltou de um envio desfeito ou com falha: conta como alterado. */
  restored?: boolean;
  /** Rascunho salvo que este painel continua; ausente = o painel cria um id novo. */
  draftId?: string;
}

interface Props {
  init: ComposeInit;
  accounts: Account[];
  onSend: (draft: ComposeInit) => void;
  onDiscard: () => void;
}

const EXIT_MS = 150;
const AUTOSAVE_MS = 1500;
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
  const [draftId] = useState(() => init.draftId ?? crypto.randomUUID());
  const [saved, setSaved] = useState(false);
  /** Última versão gravada; evita regravar o mesmo texto. */
  const savedKey = useRef("");

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

  const isBlank = !draft.to.trim() && !draft.subject.trim() && !draft.body.trim();
  const latest = useRef(draft);
  latest.current = draft;

  /** Grava o rascunho atual; texto todo em branco apaga o rascunho. Devolve false se não gravou. */
  async function persist(): Promise<boolean> {
    const d = latest.current;
    const key = JSON.stringify([d.account, d.to, d.subject, d.body]);
    if (key === savedKey.current) return true;
    try {
      if (!d.to.trim() && !d.subject.trim() && !d.body.trim()) await api.deleteDraft(draftId);
      else await api.saveDraft({ id: draftId, account: d.account, reply_to: d.reply_to, to: d.to, subject: d.subject, body: d.body });
      savedKey.current = key;
      setSaved(!!(d.to.trim() || d.subject.trim() || d.body.trim()));
      return true;
    } catch (e) {
      toast(`Não consegui salvar o rascunho: ${e}`, "error");
      return false;
    }
  }

  useEffect(() => {
    if (!dirty) return;
    const t = setTimeout(() => { persist(); }, AUTOSAVE_MS);
    return () => clearTimeout(t);
  }, [draft.account, draft.to, draft.subject, draft.body, dirty]);

  // Janela escondida ou fechando: não espera o debounce.
  useEffect(() => {
    const flush = () => { if (document.visibilityState === "hidden" && dirty) persist(); };
    document.addEventListener("visibilitychange", flush);
    return () => document.removeEventListener("visibilitychange", flush);
  });

  // Painel substituído ou desmontado com texto ainda no debounce (exceto descarte): grava antes de sumir.
  const skipFlush = useRef(false);
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  useEffect(() => () => { if (dirtyRef.current && !skipFlush.current) persist(); }, []);

  /** Fecha guardando o texto: só sai se a gravação deu certo (nunca perde o que foi digitado). */
  async function saveAndClose() {
    if (!dirty || isBlank) return leave(onDiscard);
    if (!(await persist())) return;
    toast("Rascunho salvo", "info");
    leave(onDiscard);
  }

  function discard() {
    skipFlush.current = true;
    api.deleteDraft(draftId).catch(() => {});
    leave(onDiscard);
  }

  function leave(then: () => void) {
    setLeaving(true);
    setTimeout(then, EXIT_MS);
  }

  function requestClose() {
    saveAndClose();
  }

  function requestDiscard() {
    if (dirty && !isBlank) setConfirming(true);
    else discard();
  }

  function send() {
    const problem = invalidRecipient(draft.to);
    if (problem) {
      setError(problem);
      setErrorKey((k) => k + 1);
      toRef.current?.focus();
      return;
    }
    leave(() => onSend({ ...draft, draftId }));
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
    setSaved(false);
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
        {saved && <span className="draft-saved" role="status">Rascunho salvo</span>}
        <button type="button" className="icon-btn push" aria-label="Descartar rascunho" onClick={requestDiscard}>
          <Trash size={16} weight="bold" />
        </button>
      </footer>

      {confirming && (
        <div className="confirm" role="alertdialog" aria-labelledby="discard-title">
          <p id="discard-title">Descartar rascunho?</p>
          <div className="confirm-actions">
            <button type="button" className="btn" onClick={() => setConfirming(false)} autoFocus>Continuar editando</button>
            <button type="button" className="btn danger" onClick={discard}>Descartar</button>
          </div>
        </div>
      )}
    </section>
  );
}
