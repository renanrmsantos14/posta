// Editar conta: nome, apelido e cor (só na interface), senha, servidores e portas. A conexão é testada antes de gravar; com falha nada muda.
import { useEffect, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";
import { api, type Account, type Security } from "./api";
import { ColorField } from "./AccountSteps";

interface Props {
  account: Account;
  /** Apelido salvo (vazio = padrão), o padrão que aparece no campo vazio e a cor atual. */
  label: string;
  defaultLabel: string;
  color: string;
  onLook: (label: string, color: string) => void;
  onClose: () => void;
  onSaved: (a: Account) => void;
}

const EXIT_MS = 150;

export function AccountEditDialog({ account, label, defaultLabel, color, onLook, onClose, onSaved }: Props) {
  const oauth = !!account.auth && account.auth !== "password";
  const domain = account.username.split("@")[1] ?? "";
  const start = {
    name: account.name ?? "",
    password: "",
    host: account.host,
    port: String(account.port),
    security: account.security,
    smtpHost: account.smtp?.host ?? `smtp.${domain}`,
    smtpPort: String(account.smtp?.port ?? 465),
    label,
    color,
  };
  const [form, setForm] = useState(start);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const first = useRef<HTMLInputElement>(null);

  const dirty = (Object.keys(start) as (keyof typeof start)[]).some((k) => form[k] !== start[k]);
  /** Apelido e cor não dependem do servidor: sem mudança de conexão, salvar não reconecta. */
  const connDirty = (["name", "password", "host", "port", "security", "smtpHost", "smtpPort"] as const).some((k) => form[k] !== start[k]);
  const set = <K extends keyof typeof form>(key: K, value: (typeof form)[K]) => setForm((f) => ({ ...f, [key]: value }));

  useEffect(() => first.current?.focus(), []);

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }

  function requestClose() {
    if (busy) return; // durante o teste de conexão não fecha
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
    if (busy) return;
    setError("");
    setBusy(true);
    try {
      // Apelido e cor só valem depois da conexão salva (se ela falhar, o diálogo continua aberto).
      if (connDirty) {
        const smtp = { host: form.smtpHost.trim(), port: Number(form.smtpPort), security: (form.smtpPort === "465" ? "tls" : "starttls") as Security };
        const saved = await api.updateAccount(
          { ...account, name: form.name.trim() || null, host: form.host.trim(), port: Number(form.port), security: form.security, smtp },
          form.password || null,
        );
        onSaved(saved);
      }
      onLook(form.label.trim(), form.color);
      close();
    } catch (err) {
      setError(String(err).includes("LOGIN") ? "Usuário ou senha recusados pelo servidor." : String(err));
      setErrorKey((k) => k + 1);
      setBusy(false);
    }
  }

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="edit-title" onSubmit={submit}>
        <header className="dialog-head">
          <h2 id="edit-title">Editar conta</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={busy}>
            <X size={16} weight="bold" />
          </button>
        </header>
        <p className="hint">{account.username}</p>

        <label className="field">
          <span>Nome de exibição</span>
          <input ref={first} value={form.name} onChange={(e) => set("name", e.target.value)} placeholder="Como aparece para quem recebe" />
        </label>

        <label className="field">
          <span>Apelido (aparece na barra lateral e na lista)</span>
          <input value={form.label} onChange={(e) => set("label", e.target.value)} placeholder={defaultLabel} maxLength={24} />
        </label>
        <ColorField label="Cor" color={form.color} onColor={(c) => set("color", c)} />

        {oauth ? (
          <p className="hint">O login desta conta é feito pelo provedor; aqui só dá para mudar o nome, o apelido e a cor. Para reconectar, use "Entrar de novo" na lista de contas.</p>
        ) : (
          <>
            <label className="field">
              <span>Nova senha</span>
              <input type="password" autoComplete="new-password" value={form.password} onChange={(e) => set("password", e.target.value)} placeholder="Deixe em branco para manter a atual" />
            </label>
            <div className="field-row">
              <label className="field grow">
                <span>Servidor IMAP</span>
                <input required value={form.host} onChange={(e) => set("host", e.target.value)} />
              </label>
              <label className="field port">
                <span>Porta</span>
                <input required inputMode="numeric" pattern="\d+" value={form.port} onChange={(e) => set("port", e.target.value)} />
              </label>
            </div>
            <div className="field-row">
              <label className="field grow">
                <span>Servidor SMTP (envio)</span>
                <input required value={form.smtpHost} onChange={(e) => set("smtpHost", e.target.value)} />
              </label>
              <label className="field port">
                <span>Porta</span>
                <input required inputMode="numeric" pattern="\d+" value={form.smtpPort} onChange={(e) => set("smtpPort", e.target.value)} />
              </label>
            </div>
            <label className="field">
              <span>Segurança IMAP</span>
              <select value={form.security} onChange={(e) => set("security", e.target.value as Security)}>
                <option value="tls">TLS (recomendado)</option>
                <option value="starttls">STARTTLS</option>
              </select>
            </label>
            <p className="hint">A senha fica no cofre do sistema, nunca em arquivo.</p>
          </>
        )}

        {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}

        <button className="btn primary wide" disabled={busy || !dirty}>
          {busy ? <><span className="spinner" aria-hidden />Testando conexão</> : "Salvar"}
        </button>

        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="edit-confirm">
            <p id="edit-confirm">Descartar alterações?</p>
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
