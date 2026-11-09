// Adicionar conta IMAP. Fecha por X, Esc ou clique fora; pergunta antes de descartar o que foi digitado.
import { useEffect, useRef, useState } from "react";
import { X, Check, GoogleLogo, MicrosoftOutlookLogo } from "@phosphor-icons/react";
import { api, type Account, type Provider, type Security } from "./api";

interface Props { onClose: () => void; onAdded: (a: Account) => void }

/** Servidores conhecidos pelo domínio do email. */
const OUTLOOK = { host: "outlook.office365.com", smtp: "smtp-mail.outlook.com", smtpPort: "587" };
const PRESETS: Record<string, { host: string; smtp: string; smtpPort: string; hint?: string }> = {
  "gmail.com": { host: "imap.gmail.com", smtp: "smtp.gmail.com", smtpPort: "465", hint: "No Gmail, use uma senha de app (Conta Google → Segurança)." },
  "outlook.com": OUTLOOK,
  "hotmail.com": OUTLOOK,
  "live.com": OUTLOOK,
  "icloud.com": { host: "imap.mail.me.com", smtp: "smtp.mail.me.com", smtpPort: "587", hint: "No iCloud, use uma senha específica de app." },
  "yahoo.com": { host: "imap.mail.yahoo.com", smtp: "smtp.mail.yahoo.com", smtpPort: "465" },
};

const OAUTH_BUTTONS: { provider: Provider; label: string; Icon: typeof GoogleLogo }[] = [
  { provider: "google", label: "Entrar com Google", Icon: GoogleLogo },
  { provider: "microsoft", label: "Entrar com Microsoft", Icon: MicrosoftOutlookLogo },
];

type Status = "idle" | "saving" | "done";
const EXIT_MS = 150;

export function AccountDialog({ onClose, onAdded }: Props) {
  const [form, setForm] = useState({ username: "", password: "", host: "", port: "993", security: "tls" as Security, smtpHost: "", smtpPort: "465" });
  const [hostTouched, setHostTouched] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const first = useRef<HTMLInputElement>(null);
  const [providers, setProviders] = useState<Provider[] | null>(null); // null = ainda carregando
  const [oauth, setOauth] = useState<Provider | null>(null);

  const dirty = form.username !== "" || form.password !== "" || hostTouched;
  const domain = form.username.split("@")[1]?.toLowerCase() ?? "";
  const preset = PRESETS[domain];

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
    setForm((f) => {
      const next = { ...f, [key]: value };
      if (key === "username" && !hostTouched) {
        const d = String(value).split("@")[1]?.toLowerCase();
        next.host = (d && PRESETS[d]?.host) || (d ? `imap.${d}` : "");
        next.smtpHost = (d && PRESETS[d]?.smtp) || (d ? `smtp.${d}` : "");
        next.smtpPort = (d && PRESETS[d]?.smtpPort) || "465";
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
      const { smtpHost, smtpPort, ...imap } = form;
      const smtp = { host: smtpHost, port: Number(smtpPort), security: (smtpPort === "465" ? "tls" : "starttls") as Security };
      const acc = await api.addAccount({ ...imap, port: Number(form.port), smtp });
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
                title={available ? undefined : "Indisponível nesta versão: faltam os Client IDs: Configurações > Contas > Avançado (docs/oauth-setup.md)."}
                onClick={() => signIn(provider)}>
                {oauth === provider ? <><span className="spinner" aria-hidden />Aguardando o navegador…</> : <><Icon size={18} weight="bold" />{label}</>}
              </button>
            );
          })}
          {missing && <p className="hint">Entrar com Google ou Microsoft ainda não está habilitado: informe os Client IDs em Configurações › Contas › Avançado (veja docs/oauth-setup.md).</p>}
          <p className="oauth-or"><span>ou use email e senha</span></p>
        </div>

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
        <div className="field-row">
          <label className="field grow">
            <span>Servidor SMTP (envio)</span>
            <input required value={form.smtpHost} onChange={(e) => { setHostTouched(true); update("smtpHost", e.target.value); }} />
          </label>
          <label className="field port">
            <span>Porta</span>
            <input required inputMode="numeric" pattern="\d+" value={form.smtpPort} onChange={(e) => update("smtpPort", e.target.value)} />
          </label>
        </div>
        <label className="field">
          <span>Segurança IMAP</span>
          <select value={form.security} onChange={(e) => update("security", e.target.value as Security)}>
            <option value="tls">TLS (recomendado)</option>
            <option value="starttls">STARTTLS</option>
          </select>
        </label>

        {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
        <p className="hint">A senha fica no cofre do sistema, nunca em arquivo.</p>

        <button className="btn primary wide" data-status={status} disabled={busy}>
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
