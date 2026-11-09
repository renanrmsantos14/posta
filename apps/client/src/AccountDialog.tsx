// Adicionar conta IMAP. Fecha por X, Esc ou clique fora; pergunta antes de descartar o que foi digitado.
import { useEffect, useRef, useState } from "react";
import { X, Check, CaretRight, GoogleLogo, MicrosoftOutlookLogo } from "@phosphor-icons/react";
import { api, type Account, type Detected, type Provider, type Security } from "./api";

interface Props { onClose: () => void; onAdded: (a: Account) => void }

const OAUTH_BUTTONS: { provider: Provider; label: string; Icon: typeof GoogleLogo }[] = [
  { provider: "google", label: "Entrar com Google", Icon: GoogleLogo },
  { provider: "microsoft", label: "Entrar com Microsoft", Icon: MicrosoftOutlookLogo },
];

type Status = "idle" | "detecting" | "saving" | "done";
const EMPTY_FORM = { username: "", password: "", host: "", port: "993", security: "tls" as Security, smtpHost: "", smtpPort: "465", smtpSecurity: "tls" as Security };
const plainError = (err: unknown) => String(err).replace(/^Error: /, "");
const EXIT_MS = 150;

export function AccountDialog({ onClose, onAdded }: Props) {
  const [email, setEmail] = useState("");
  const [form, setForm] = useState(EMPTY_FORM);
  const [detected, setDetected] = useState<{ email: string; info: Detected } | null>(null);
  const [advanced, setAdvanced] = useState(false);
  const [hostTouched, setHostTouched] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const first = useRef<HTMLInputElement>(null);
  const [providers, setProviders] = useState<Provider[] | null>(null); // null = ainda carregando
  const [oauth, setOauth] = useState<Provider | null>(null);

  const dirty = email !== "" || form.password !== "" || hostTouched;
  const info = detected?.email === email.trim() ? detected.info : null;

  useEffect(() => first.current?.focus(), []);
  useEffect(() => { api.oauthProviders().then(setProviders).catch(() => setProviders([])); }, []);

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }

  function requestClose() {
    if (status !== "idle" || oauth) return; // durante o salvamento e a espera do navegador não fecha
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
    setForm((f) => ({ ...f, [key]: value }));
  }

  /** Detecta os servidores; preenche os campos avançados, exceto os que o usuário já editou. */
  async function detect(): Promise<Detected | null> {
    const addr = email.trim();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(addr)) return null;
    if (detected?.email === addr) return detected.info;
    setStatus("detecting");
    try {
      const d = await api.detectServers(addr);
      setDetected({ email: addr, info: d });
      if (!hostTouched) {
        setForm((f) => ({ ...f, username: d.username, host: d.imap.host, port: String(d.imap.port), security: d.imap.security,
          smtpHost: d.smtp.host, smtpPort: String(d.smtp.port), smtpSecurity: d.smtp.security }));
      } else setForm((f) => ({ ...f, username: d.username }));
      return d;
    } catch (err) {
      setError(plainError(err));
      setErrorKey((k) => k + 1);
      return null;
    } finally {
      setStatus("idle");
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    if (!info && !(await detect())) return;
    setStatus("saving");
    try {
      const { smtpHost, smtpPort, smtpSecurity, ...imap } = form;
      const smtp = { host: smtpHost, port: Number(smtpPort), security: smtpSecurity };
      const acc = await api.addAccount({ ...imap, port: Number(form.port), smtp });
      setStatus("done");
      setTimeout(() => {
        onAdded(acc);
        close();
      }, 650);
    } catch (err) {
      setStatus("idle");
      setAdvanced(true);
      setError(plainError(err));
      setErrorKey((k) => k + 1);
    }
  }

  async function signIn(provider: Provider) {
    setError("");
    setOauth(provider);
    try {
      const acc = await api.oauthLogin(provider);
      setStatus("done");
      setTimeout(() => {
        onAdded(acc);
        close();
      }, 650);
    } catch (err) {
      setError(String(err));
      setErrorKey((k) => k + 1);
    } finally {
      setOauth(null);
    }
  }

  const busy = status !== "idle" || oauth !== null;
  const missing = providers !== null && OAUTH_BUTTONS.some((b) => !providers.includes(b.provider));

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog" role="dialog" aria-modal="true" aria-labelledby="dlg-title" onSubmit={submit}>
        <header className="dialog-head">
          <h2 id="dlg-title">Adicionar conta</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={busy}>
            <X size={16} weight="bold" />
          </button>
        </header>

        <div className="oauth-buttons">
          {OAUTH_BUTTONS.map(({ provider, label, Icon }) => {
            const available = providers?.includes(provider) ?? false;
            return (
              <button key={provider} type="button" className="btn wide" disabled={!available || busy}
                title={available ? undefined : "Indisponível nesta versão: faltam as credenciais do app (docs/oauth.md)."}
                onClick={() => signIn(provider)}>
                {oauth === provider ? <><span className="spinner" aria-hidden />Aguardando o navegador…</> : <><Icon size={18} weight="bold" />{label}</>}
              </button>
            );
          })}
          {missing && <p className="hint">Entrar com Google ou Microsoft ainda não está habilitado nesta versão (veja docs/oauth.md).</p>}
          <p className="oauth-or"><span>ou use email e senha</span></p>
        </div>

        <label className="field">
          <span>Email</span>
          <input ref={first} type="email" required autoComplete="username" value={email} onBlur={() => { if (!busy) void detect(); }}
            onChange={(e) => { setEmail(e.target.value); setError(""); }} placeholder="voce@gmail.com" />
        </label>
        <label className="field">
          <span>Senha</span>
          <input type="password" required autoComplete="current-password" value={form.password}
            onChange={(e) => update("password", e.target.value)} />
        </label>

        <p className="detected" aria-live="polite">
          {status === "detecting" ? <><span className="spinner" aria-hidden />Detectando…</>
            : info ? <><Check size={14} weight="bold" />{info.provider ? `Detectado: ${info.provider}` : "Servidores não identificados: usei o padrão do domínio"}</> : null}
        </p>
        {info?.hint && (
          <p className="hint detected-hint">
            {info.hint.text}{" "}
            {info.hint.url && <a href={info.hint.url} target="_blank" rel="noreferrer">Abrir página oficial</a>}
          </p>
        )}

        <button type="button" className="adv-toggle" aria-expanded={advanced} onClick={() => setAdvanced((v) => !v)}>
          <CaretRight size={12} weight="bold" className="adv-caret" />Configurações avançadas
        </button>
        {advanced && (
          <div className="adv">
            <label className="field">
              <span>Usuário (login)</span>
              <input required value={form.username} onChange={(e) => { setHostTouched(true); update("username", e.target.value); }} />
            </label>
            <div className="field-row">
              <label className="field grow">
                <span>Servidor IMAP</span>
                <input required value={form.host} onChange={(e) => { setHostTouched(true); update("host", e.target.value); }} />
              </label>
              <label className="field port">
                <span>Porta</span>
                <input required inputMode="numeric" pattern="\d+" value={form.port} onChange={(e) => { setHostTouched(true); update("port", e.target.value); }} />
              </label>
            </div>
            <label className="field">
              <span>Segurança IMAP</span>
              <select value={form.security} onChange={(e) => { setHostTouched(true); update("security", e.target.value as Security); }}>
                <option value="tls">TLS (recomendado)</option>
                <option value="starttls">STARTTLS</option>
              </select>
            </label>
            <div className="field-row">
              <label className="field grow">
                <span>Servidor SMTP (envio)</span>
                <input required value={form.smtpHost} onChange={(e) => { setHostTouched(true); update("smtpHost", e.target.value); }} />
              </label>
              <label className="field port">
                <span>Porta</span>
                <input required inputMode="numeric" pattern="\d+" value={form.smtpPort} onChange={(e) => { setHostTouched(true); update("smtpPort", e.target.value); }} />
              </label>
            </div>
            <label className="field">
              <span>Segurança SMTP</span>
              <select value={form.smtpSecurity} onChange={(e) => { setHostTouched(true); update("smtpSecurity", e.target.value as Security); }}>
                <option value="tls">TLS (recomendado)</option>
                <option value="starttls">STARTTLS</option>
              </select>
            </label>
          </div>
        )}

        {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
        <p className="hint">A senha fica no cofre do sistema, nunca em arquivo.</p>

        <button className="btn primary wide" data-status={status === "detecting" ? "saving" : status} disabled={busy}>
          <span className="btn-label" data-show={status === "idle" || undefined}>Conectar</span>
          <span className="btn-label" data-show={status === "saving" || status === "detecting" || undefined}><span className="spinner" aria-hidden />Testando conexão…</span>
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
