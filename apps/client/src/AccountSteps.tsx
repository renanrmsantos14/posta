// Etapas do assistente de conta (apresentação). O estado e as chamadas ficam em AccountDialog.
import { Check, GoogleLogo, MicrosoftOutlookLogo, X } from "@phosphor-icons/react";
import type { CheckFailure, CheckStep, Hosting, Provider, Security } from "./api";
import { ACCOUNT_COLORS } from "./accountColors";
import { STEP_LABEL, explain } from "./accountSetup";
import { openExternal } from "./HtmlBody";

export interface ServerForm { host: string; port: string; security: Security; smtpHost: string; smtpPort: string }

const OAUTH_ICON = { google: GoogleLogo, microsoft: MicrosoftOutlookLogo };
const OAUTH_LABEL: Record<Provider, string> = { google: "Entrar com Google", microsoft: "Entrar com Microsoft" };

export function OauthButton({ provider, waiting, disabled, primary, onClick }: { provider: Provider; waiting: Provider | null; disabled: boolean; primary?: boolean; onClick: () => void }) {
  const Icon = OAUTH_ICON[provider];
  return (
    <button type="button" className={`btn wide${primary ? " primary" : ""}`} disabled={disabled} onClick={onClick}>
      {waiting === provider ? <><span className="spinner" aria-hidden />Aguardando o navegador…</> : <><Icon size={18} weight="bold" />{OAUTH_LABEL[provider]}</>}
    </button>
  );
}

/** Passo a passo da senha de app do Gmail: a senha normal não funciona fora do site do Google. */
export function GmailGuide() {
  return (
    <ol className="guide">
      <li>Ative a verificação em duas etapas na sua conta Google.</li>
      <li><button type="button" className="link-btn" onClick={() => openExternal("https://myaccount.google.com/apppasswords")}>Gerar senha de app</button> (16 letras).</li>
      <li>Cole abaixo. Os espaços podem ficar, o Posta os remove.</li>
    </ol>
  );
}

export function ManualServers({ form, open, onToggle, onChange }: { form: ServerForm; open: boolean; onToggle: (open: boolean) => void; onChange: (patch: Partial<ServerForm>) => void }) {
  return (
    <details className="manual" open={open} onToggle={(e) => onToggle(e.currentTarget.open)}>
      <summary>Configuração manual</summary>
      <div className="manual-body">
        <div className="field-row">
          <label className="field grow">
            <span>Servidor IMAP</span>
            <input value={form.host} onChange={(e) => onChange({ host: e.target.value })} />
          </label>
          <label className="field port">
            <span>Porta</span>
            <input inputMode="numeric" value={form.port} onChange={(e) => onChange({ port: e.target.value })} />
          </label>
        </div>
        <div className="field-row">
          <label className="field grow">
            <span>Servidor SMTP (envio)</span>
            <input value={form.smtpHost} onChange={(e) => onChange({ smtpHost: e.target.value })} />
          </label>
          <label className="field port">
            <span>Porta</span>
            <input inputMode="numeric" value={form.smtpPort} onChange={(e) => onChange({ smtpPort: e.target.value })} />
          </label>
        </div>
        <label className="field">
          <span>Segurança IMAP</span>
          <select value={form.security} onChange={(e) => onChange({ security: e.target.value as Security })}>
            <option value="tls">TLS (recomendado)</option>
            <option value="starttls">STARTTLS</option>
          </select>
        </label>
      </div>
    </details>
  );
}

const STEPS: CheckStep[] = ["connect", "login", "smtp"];

interface FailCtx { hosting: Hosting; form: ServerForm; canMicrosoft: boolean }

/** O que houve e o que fazer; o erro cru só aparece em "Detalhes". */
export function FailureBox({ failure, hosting, form, canMicrosoft }: { failure: CheckFailure } & FailCtx) {
  const smtp = failure.step === "smtp";
  const msg = explain(failure, { hosting, host: smtp ? form.smtpHost : form.host, port: Number(smtp ? form.smtpPort : form.port), canMicrosoft });
  return (
    <div className="failure" role="alert">
      <strong>{msg.title}</strong>
      <p>{msg.hint}</p>
      {failure.detail && <details><summary>Detalhes</summary><code>{failure.detail}</code></details>}
    </div>
  );
}

/** Progresso do teste: cada etapa vira ✓ ao passar; a que falhou fica ✗ com a explicação logo abaixo. */
export function TestProgress({ reached, failure, busy, onFix, onRetry, onMicrosoft, ...ctx }: {
  reached: number; failure: CheckFailure | null; busy: boolean;
  onFix: () => void; onRetry: () => void; onMicrosoft: () => void;
} & FailCtx) {
  const failIdx = failure ? STEPS.indexOf(failure.step) : -1;
  const state = (i: number) => (failIdx >= 0 ? (i < failIdx ? "ok" : i === failIdx ? "fail" : "wait") : i < reached ? "ok" : i === reached ? "run" : "wait");
  return (
    <div className="wizard-step">
      <ol className="progress" aria-live="polite">
        {STEPS.map((s, i) => {
          const st = state(i);
          return (
            <li key={s} data-state={st}>
              <span className="mark" aria-hidden>
                {st === "ok" && <Check size={12} weight="bold" />}
                {st === "run" && <span className="spinner" />}
                {st === "fail" && <X size={12} weight="bold" />}
              </span>
              {STEP_LABEL[s]}
              <span className="sr-only">{st === "ok" ? ": ok" : st === "fail" ? ": falhou" : st === "run" ? ": em andamento" : ""}</span>
            </li>
          );
        })}
      </ol>
      {failure && <FailureBox failure={failure} {...ctx} />}
      {failure && (
        <div className="dialog-actions">
          {failure.kind === "auth" && ctx.hosting === "microsoft" && ctx.canMicrosoft && (
            <button type="button" className="btn" disabled={busy} onClick={onMicrosoft}><MicrosoftOutlookLogo size={16} weight="bold" />Entrar com Microsoft</button>
          )}
          <button type="button" className="btn primary" onClick={onFix} autoFocus>Corrigir</button>
          <button type="button" className="btn" onClick={onRetry}>Tentar de novo</button>
        </div>
      )}
    </div>
  );
}

export function Personalize({ email, name, signature, color, onName, onSignature, onColor }: {
  email: string; name: string; signature: string; color: string;
  onName: (v: string) => void; onSignature: (v: string) => void; onColor: (v: string) => void;
}) {
  return (
    <div className="wizard-step">
      <div className="preview" aria-hidden>
        <span className="avatar large" data-color={color}>{(name.trim() || email)[0]?.toUpperCase()}</span>
        <div><b>{name.trim() || email}</b><span>{email}</span></div>
      </div>
      <label className="field">
        <span>Nome de exibição</span>
        <input value={name} onChange={(e) => onName(e.target.value)} placeholder="Como aparece para quem recebe" autoFocus />
      </label>
      <label className="field">
        <span>Assinatura (opcional)</span>
        <textarea className="signature-input" rows={3} value={signature} onChange={(e) => onSignature(e.target.value)} placeholder="Entra no fim das mensagens novas e das respostas" />
      </label>
      <ColorField label="Cor na barra lateral" color={color} onColor={onColor} />
    </div>
  );
}

/** Escolha da cor da conta (paleta de accountColors.ts). */
export function ColorField({ label, color, onColor }: { label: string; color: string; onColor: (id: string) => void }) {
  return (
    <div className="field" role="radiogroup" aria-label="Cor da conta">
      <span>{label}</span>
      <div className="swatches">
        {ACCOUNT_COLORS.map((c) => (
          <button key={c.id} type="button" role="radio" aria-checked={color === c.id} aria-label={c.label} title={c.label} className="swatch" onClick={() => onColor(c.id)}>
            <span className="avatar" data-color={c.id} />
          </button>
        ))}
      </div>
    </div>
  );
}
