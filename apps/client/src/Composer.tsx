// Escrever e responder. Painel fixo, sem cobrir a mensagem que está sendo respondida.
// Esc ou X perguntam antes de descartar; Ctrl/Cmd+Enter envia.
import { useEffect, useRef, useState } from "react";
import { File as FileIcon, Paperclip, PaperPlaneTilt, Trash, X } from "@phosphor-icons/react";
import type { Account, Draft, OutAttachment } from "./api";
import { fmtSize } from "./format";

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

/** Limite do total anexado (o servidor recusa acima de 25 MB). */
const MAX_BYTES = 25 * 1024 * 1024;

/** Cada destinatário precisa ter algo@algo. */
function badAddress(list: string): string | undefined {
  return list.split(/[,;]/).map((s) => s.trim()).filter(Boolean).find((s) => !/[^\s<@]+@[^\s>@]+\.[^\s>@]+/.test(s));
}

function invalidRecipient(to: string): string | null {
  if (!to.split(/[,;]/).some((s) => s.trim())) return "Informe para quem vai a mensagem.";
  const bad = badAddress(to);
  return bad ? `Endereço inválido: ${bad}` : null;
}

/** Bytes do arquivo a partir do base64. */
const b64Size = (data: string) => Math.floor((data.length * 3) / 4) - (data.endsWith("==") ? 2 : data.endsWith("=") ? 1 : 0);

function readBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).slice(String(r.result).indexOf(",") + 1));
    r.onerror = () => reject(r.error);
    r.readAsDataURL(file);
  });
}

export function Composer({ init, accounts, onSend, onDiscard }: Props) {
  const [draft, setDraft] = useState(init);
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [showCc, setShowCc] = useState(!!(init.cc || init.bcc));
  const [attachNote, setAttachNote] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);
  const toRef = useRef<HTMLInputElement>(null);
  const bodyRef = useRef<HTMLTextAreaElement>(null);

  const dirty = !!init.restored || draft.to !== init.to || draft.subject !== init.subject || draft.body !== init.body
    || (draft.cc ?? "") !== (init.cc ?? "") || (draft.bcc ?? "") !== (init.bcc ?? "")
    || (draft.attachments?.length ?? 0) !== (init.attachments?.length ?? 0);
  const files = draft.attachments ?? [];
  const total = files.reduce((n, f) => n + b64Size(f.data), 0);

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
    const badCopy = badAddress(draft.cc ?? "") ?? badAddress(draft.bcc ?? "");
    if (badCopy) {
      setShowCc(true);
      setError(`Endereço inválido: ${badCopy}`);
      setErrorKey((k) => k + 1);
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
    if (k === "to" || k === "cc" || k === "bcc") setError("");
  };

  async function addFiles(list: FileList | null) {
    const picked = Array.from(list ?? []);
    if (fileRef.current) fileRef.current.value = "";
    let sum = total;
    const added: OutAttachment[] = [];
    const refused: string[] = [];
    for (const f of picked) {
      if (sum + f.size > MAX_BYTES) { refused.push(f.name); continue; }
      try {
        added.push({ name: f.name, mime: f.type || "application/octet-stream", data: await readBase64(f) });
        sum += f.size;
      } catch {
        refused.push(f.name);
      }
    }
    if (added.length) setDraft((d) => ({ ...d, attachments: [...(d.attachments ?? []), ...added] }));
    setAttachNote(refused.length ? `Não anexado (limite de 25 MB no total ou falha na leitura): ${refused.join(", ")}` : "");
  }

  const removeFile = (i: number) => {
    setDraft((d) => ({ ...d, attachments: (d.attachments ?? []).filter((_, j) => j !== i) }));
    setAttachNote("");
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
          {!showCc && (
            <button type="button" className="btn ghost small cc-toggle" onClick={() => setShowCc(true)}>Cc/Bcc</button>
          )}
        </label>
        {showCc && (
          <>
            <label className="line">
              <span>Cc</span>
              <input type="text" inputMode="email" autoComplete="email" value={draft.cc ?? ""}
                onChange={(e) => set("cc", e.target.value)} placeholder="separe vários por vírgula" />
            </label>
            <label className="line">
              <span>Bcc</span>
              <input type="text" inputMode="email" autoComplete="email" value={draft.bcc ?? ""}
                onChange={(e) => set("bcc", e.target.value)} placeholder="cópia oculta" />
            </label>
          </>
        )}
        {error && <p key={errorKey} className="form-error composer-error" role="alert">{error}</p>}
        <label className="line">
          <span>Assunto</span>
          <input type="text" value={draft.subject} onChange={(e) => set("subject", e.target.value)} />
        </label>
      </div>

      <textarea ref={bodyRef} className="composer-body" value={draft.body} aria-label="Mensagem"
        onChange={(e) => set("body", e.target.value)} />

      {(files.length > 0 || attachNote) && (
        <div className="attach-bar">
          {files.length > 0 && (
            <ul className="chips" aria-label="Anexos">
              {files.map((f, i) => (
                <li key={`${f.name}-${i}`} className="chip">
                  <FileIcon size={14} weight="bold" aria-hidden />
                  <span className="chip-name" title={f.name}>{f.name}</span>
                  <span className="chip-size">{fmtSize(b64Size(f.data))}</span>
                  <button type="button" className="chip-x" aria-label={`Remover ${f.name}`} onClick={() => removeFile(i)}>
                    <X size={12} weight="bold" />
                  </button>
                </li>
              ))}
            </ul>
          )}
          {files.length > 0 && <span className="attach-total">{fmtSize(total)} de 25 MB</span>}
          {attachNote && <p className="form-error" role="alert">{attachNote}</p>}
        </div>
      )}

      <footer className="composer-foot">
        <button type="button" className="btn primary" onClick={send}>
          <PaperPlaneTilt size={15} weight="fill" /> Enviar
        </button>
        <input ref={fileRef} type="file" multiple hidden onChange={(e) => void addFiles(e.target.files)} />
        <button type="button" className="icon-btn" aria-label="Anexar arquivos" title="Anexar arquivos"
          onClick={() => fileRef.current?.click()}>
          <Paperclip size={16} weight="bold" />
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
