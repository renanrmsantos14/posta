// Adicionar conta IMAP. Fecha por X, Esc ou clique fora; pergunta antes de descartar o que foi digitado.
import { useEffect, useRef, useState } from "react";
import { X, Check } from "@phosphor-icons/react";
import { api, type Account, type Security } from "./api";

interface Props { onClose: () => void; onAdded: (a: Account) => void }

/** Servidores conhecidos pelo domínio do email. */
const PRESETS: Record<string, { host: string; hint?: string }> = {
  "gmail.com": { host: "imap.gmail.com", hint: "No Gmail, use uma senha de app (Conta Google → Segurança)." },
  "outlook.com": { host: "outlook.office365.com" },
  "hotmail.com": { host: "outlook.office365.com" },
  "live.com": { host: "outlook.office365.com" },
  "icloud.com": { host: "imap.mail.me.com", hint: "No iCloud, use uma senha específica de app." },
  "yahoo.com": { host: "imap.mail.yahoo.com" },
};

type Status = "idle" | "saving" | "done";
const EXIT_MS = 150;

export function AccountDialog({ onClose, onAdded }: Props) {
  const [form, setForm] = useState({ username: "", password: "", host: "", port: "993", security: "tls" as Security });
  const [hostTouched, setHostTouched] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const first = useRef<HTMLInputElement>(null);

  const dirty = form.username !== "" || form.password !== "" || hostTouched;
  const domain = form.username.split("@")[1]?.toLowerCase() ?? "";
  const preset = PRESETS[domain];

  useEffect(() => first.current?.focus(), []);

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }

  function requestClose() {
    if (status !== "idle") return; // durante o salvamento não fecha
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

  function update<K extends keyof typeof form>(key: K, value: (typeof form)[K]) {
    setForm((f) => {
      const next = { ...f, [key]: value };
      if (key === "username" && !hostTouched) {
        const d = String(value).split("@")[1]?.toLowerCase();
        next.host = (d && PRESETS[d]?.host) || (d ? `imap.${d}` : "");
      }
      if (key === "security") next.port = value === "tls" ? "993" : "143";
      return next;
    });
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setStatus("saving");
    try {
      const acc = await api.addAccount({ ...form, port: Number(form.port) });
      setStatus("done");
      setTimeout(() => {
        onAdded(acc);
        close();
      }, 650);
    } catch (err) {
      setStatus("idle");
      setError(String(err).includes("LOGIN") ? "Usuário ou senha recusados pelo servidor." : String(err));
      setErrorKey((k) => k + 1);
    }
  }

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="dlg-title" onSubmit={submit}>
        <header className="dialog-head">
          <h2 id="dlg-title">Adicionar conta</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={status !== "idle"}>
            <X size={16} weight="bold" />
          </button>
        </header>

        <label className="field">
          <span>Email</span>
          <input ref={first} type="email" required autoComplete="username" value={form.username}
            onChange={(e) => update("username", e.target.value)} placeholder="voce@gmail.com" />
        </label>
        <label className="field">
          <span>Senha</span>
          <input type="password" required autoComplete="current-password" value={form.password}
            onChange={(e) => update("password", e.target.value)} />
        </label>
        {preset?.hint && <p className="hint">{preset.hint}</p>}

        <div className="field-row">
          <label className="field grow">
            <span>Servidor IMAP</span>
            <input required value={form.host} onChange={(e) => { setHostTouched(true); update("host", e.target.value); }} />
          </label>
          <label className="field port">
            <span>Porta</span>
            <input required inputMode="numeric" pattern="\d+" value={form.port} onChange={(e) => update("port", e.target.value)} />
          </label>
        </div>
        <label className="field">
          <span>Segurança</span>
          <select value={form.security} onChange={(e) => update("security", e.target.value as Security)}>
            <option value="tls">TLS (recomendado)</option>
            <option value="starttls">STARTTLS</option>
          </select>
        </label>

        {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
        <p className="hint">A senha fica no cofre do sistema, nunca em arquivo.</p>

        <button className="btn primary wide" data-status={status} disabled={status !== "idle"}>
          <span className="btn-label" data-show={status === "idle" || undefined}>Conectar</span>
          <span className="btn-label" data-show={status === "saving" || undefined}><span className="spinner" aria-hidden />Testando conexão</span>
          <span className="btn-label" data-show={status === "done" || undefined}><Check size={16} weight="bold" />Conectada</span>
        </button>

        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="confirm-title">
            <p id="confirm-title">Descartar alterações?</p>
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
