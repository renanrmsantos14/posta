// Assistente para adicionar conta: Email → Login → Teste → Personalizar. Fecha por X, Esc ou clique fora;
// pergunta antes de descartar o que foi digitado e não fecha durante o teste, a espera do navegador e o salvamento.
import { useEffect, useState } from "react";
import { ArrowLeft, Check, X } from "@phosphor-icons/react";
import { api, type Account, type CheckFailure, type CheckStep, type NewAccount, type Provider } from "./api";
import { ACCOUNT_COLORS, colorKey } from "./accountColors";
import { asFailure, detectSetup, isServerFailure, type Setup } from "./accountSetup";
import { FailureBox, GmailGuide, ManualServers, OauthButton, Personalize, TestProgress, type ServerForm } from "./AccountSteps";

interface Props {
  onClose: () => void;
  /** A conta existe: registra na lista do app. */
  onAdded: (a: Account) => void;
  /** Assistente concluído: abre a Caixa de Entrada da conta nova. */
  onDone: (a: Account) => void;
  onSetting: (key: string, value: string) => void;
}

type Stage = "email" | "login" | "test" | "custom";
/** Conta ainda não gravada (senha, só guardada ao concluir) ou já criada pelo navegador (OAuth). */
type Pending = { kind: "password"; account: NewAccount } | { kind: "oauth"; account: Account };

const EXIT_MS = 150;
const STEPS: CheckStep[] = ["connect", "login", "smtp"];
const TITLE: Record<Stage, string> = { email: "Adicionar conta", login: "Entrar na conta", test: "Testando a conexão", custom: "Personalizar" };
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function AccountDialog({ onClose, onAdded, onDone, onSetting }: Props) {
  const [stage, setStage] = useState<Stage>("email");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [setup, setSetup] = useState<Setup | null>(null);
  const [form, setForm] = useState<ServerForm>({ host: "", port: "993", security: "tls", smtpHost: "", smtpPort: "465" });
  const [manualOpen, setManualOpen] = useState(false);
  const [usePassword, setUsePassword] = useState(false);
  const [existing, setExisting] = useState<Account[]>([]);
  const [providers, setProviders] = useState<Provider[] | null>(null); // null = ainda carregando
  const [oauth, setOauth] = useState<Provider | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [reached, setReached] = useState(0);
  const [failure, setFailure] = useState<CheckFailure | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [name, setName] = useState("");
  const [signature, setSignature] = useState("");
  const [color, setColor] = useState<string>(ACCOUNT_COLORS[0].id);
  const [saving, setSaving] = useState<"idle" | "saving" | "done">("idle");
  const [error, setError] = useState("");
  const [errorKey, setErrorKey] = useState(0);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => { api.oauthProviders().then(setProviders).catch(() => setProviders([])); }, []);

  const testing = stage === "test" && !failure;
  const busy = testing || oauth !== null || saving !== "idle" || detecting;
  const hosting = setup?.hosting ?? "other";
  const canMicrosoft = providers?.includes("microsoft") ?? false;
  // O que foi digitado só se perde se ninguém gravou a conta ainda.
  const dirty = stage === "custom" ? pending?.kind === "password" || name !== "" || signature !== "" : email !== "" || password !== "";

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }

  function requestClose() {
    if (testing || oauth || saving !== "idle") return;
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

  function fail(message: string) {
    setError(message);
    setErrorKey((k) => k + 1);
  }

  async function submitEmail(e: React.FormEvent) {
    e.preventDefault();
    const address = email.trim();
    setError("");
    setDetecting(true);
    const accounts = await api.listAccounts().catch(() => [] as Account[]);
    setExisting(accounts);
    if (accounts.some((a) => a.username.toLowerCase() === address.toLowerCase())) {
      setDetecting(false);
      fail("Esta conta já está cadastrada no Posta.");
      return;
    }
    const s = await detectSetup(address);
    setEmail(address);
    setSetup(s);
    setForm({ host: s.imap.host, port: String(s.imap.port), security: s.imap.security === "starttls" ? "starttls" : "tls", smtpHost: s.smtp.host, smtpPort: String(s.smtp.port) });
    setManualOpen(false);
    setUsePassword(false);
    setFailure(null);
    setDetecting(false);
    setStage("login");
  }

  const patchForm = (patch: Partial<ServerForm>) =>
    setForm((f) => {
      const next = { ...f, ...patch };
      if (patch.security) next.port = patch.security === "tls" ? "993" : "143";
      return next;
    });

  function buildAccount(): NewAccount {
    return {
      username: email,
      // Senha de app do Google vem em grupos separados por espaço.
      password: hosting === "google" ? password.replace(/\s+/g, "") : password,
      host: form.host.trim(),
      port: Number(form.port),
      security: form.security,
      smtp: { host: form.smtpHost.trim(), port: Number(form.smtpPort), security: form.smtpPort === "465" ? "tls" : "starttls" },
    };
  }

  async function runTest(e?: React.FormEvent) {
    e?.preventDefault();
    if (!form.host.trim() || !form.smtpHost.trim() || !/^\d+$/.test(form.port) || !/^\d+$/.test(form.smtpPort)) {
      setManualOpen(true);
      fail("Preencha os servidores e as portas em Configuração manual.");
      return;
    }
    const account = buildAccount();
    setError("");
    setFailure(null);
    setReached(0);
    setStage("test");
    for (const step of STEPS) {
      try {
        await api.checkAccount(account, step);
      } catch (err) {
        setFailure(asFailure(err));
        return;
      }
      setReached((r) => r + 1);
    }
    await wait(450); // deixa o último ✓ aparecer
    toCustomize({ kind: "password", account });
  }

  function toCustomize(p: Pending) {
    setPending(p);
    setName(p.account.name ?? "");
    setColor(ACCOUNT_COLORS[existing.length % ACCOUNT_COLORS.length].id);
    setStage("custom");
  }

  async function signIn(provider: Provider) {
    setError("");
    setFailure(null);
    setStage("login");
    setOauth(provider);
    try {
      const acc = await api.oauthLogin(provider);
      onAdded(acc);
      toCustomize({ kind: "oauth", account: acc });
    } catch (err) {
      fail(String(err));
    } finally {
      setOauth(null);
    }
  }

  async function finish(e: React.FormEvent) {
    e.preventDefault();
    if (!pending) return;
    setSaving("saving");
    setError("");
    try {
      const display = name.trim() || null;
      const acc = pending.kind === "password"
        ? await api.addAccount({ ...pending.account, name: display })
        : display ? await api.updateAccount({ ...pending.account, name: display }, null) : pending.account;
      if (signature.trim()) onSetting(`signature:${acc.id}`, signature);
      onSetting(colorKey(acc.id), color);
      onAdded(acc);
      setSaving("done");
      await wait(650);
      onDone(acc);
      close();
    } catch (err) {
      setSaving("idle");
      fail(`Não consegui salvar a conta: ${err}`);
    }
  }

  function back() {
    setError("");
    setFailure(null);
    setStage("email");
  }

  const ctx = { hosting, form, canMicrosoft };
  const microsoftFirst = hosting === "microsoft" && canMicrosoft && !usePassword;

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <div className="dialog wizard" role="dialog" aria-modal="true" aria-labelledby="dlg-title">
        <header className="dialog-head">
          <h2 id="dlg-title">{TITLE[stage]}</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={busy && !detecting}>
            <X size={16} weight="bold" />
          </button>
        </header>

        {stage === "email" && (
          <form className="wizard-step" key="email" onSubmit={submitEmail}>
            <label className="field">
              <span>Qual é o seu email?</span>
              <input type="email" required autoFocus autoComplete="username" value={email} onChange={(e) => { setEmail(e.target.value); setError(""); }} placeholder="voce@gmail.com" />
            </label>
            <p className="hint">O Posta descobre sozinho o servidor. Só o domínio é consultado, nunca a senha.</p>
            {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
            <button className="btn primary wide" disabled={detecting}>
              {detecting ? <><span className="spinner" aria-hidden />Procurando seu provedor…</> : "Continuar"}
            </button>
          </form>
        )}

        {stage === "login" && setup && (
          <form className="wizard-step" key="login" onSubmit={runTest}>
            <p className="wizard-sub">{email}</p>
            {microsoftFirst ? (
              <>
                <OauthButton provider="microsoft" waiting={oauth} disabled={busy} primary onClick={() => signIn("microsoft")} />
                <button type="button" className="link-btn" onClick={() => setUsePassword(true)}>Usar senha em vez disso</button>
              </>
            ) : (
              <>
                {hosting === "google" && <GmailGuide />}
                {hosting === "microsoft" && <p className="hint">A Microsoft costuma exigir uma senha de app (com verificação em duas etapas) em vez da senha normal.</p>}
                <label className="field">
                  <span>{hosting === "google" ? "Senha de app" : "Senha"}</span>
                  <input type="password" required autoFocus autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
                </label>
                {hosting === "google" && providers?.includes("google") && (
                  <OauthButton provider="google" waiting={oauth} disabled={busy} onClick={() => signIn("google")} />
                )}
                <ManualServers form={form} open={manualOpen} onToggle={setManualOpen} onChange={patchForm} />
                {failure && <FailureBox failure={failure} {...ctx} />}
                {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
                <p className="hint">A senha fica no cofre do sistema, nunca em arquivo.</p>
                <button className="btn primary wide" disabled={busy}>Conectar</button>
              </>
            )}
            {microsoftFirst && error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
            <button type="button" className="btn ghost small" onClick={back} disabled={busy}><ArrowLeft size={14} weight="bold" />Trocar email</button>
          </form>
        )}

        {stage === "test" && (
          <TestProgress reached={reached} failure={failure} busy={busy} {...ctx}
            onFix={() => { setManualOpen(!!failure && isServerFailure(failure)); setStage("login"); }}
            onRetry={() => void runTest()} onMicrosoft={() => signIn("microsoft")} />
        )}

        {stage === "custom" && pending && (
          <form className="wizard-step" key="custom" onSubmit={finish}>
            <Personalize email={pending.account.username} name={name} signature={signature} color={color}
              onName={setName} onSignature={setSignature} onColor={setColor} />
            {error && <p key={errorKey} className="form-error" role="alert">{error}</p>}
            <button className="btn primary wide" data-status={saving} disabled={saving !== "idle"}>
              <span className="btn-label" data-show={saving === "idle" || undefined}>Concluir</span>
              <span className="btn-label" data-show={saving === "saving" || undefined}><span className="spinner" aria-hidden />Salvando</span>
              <span className="btn-label" data-show={saving === "done" || undefined}><Check size={16} weight="bold" />Pronto</span>
            </button>
          </form>
        )}

        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="confirm-title">
            <p id="confirm-title">Descartar alterações?</p>
            <div className="confirm-actions">
              <button type="button" className="btn" onClick={() => setConfirming(false)} autoFocus>Continuar editando</button>
              <button type="button" className="btn danger" onClick={close}>Descartar</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
