// Avançado das contas: Client IDs do login com Google e Microsoft (valem mais que os embutidos no build).
import { useEffect, useState } from "react";
import { api, type OauthConfig, type OauthConfigView } from "./api";
import { toast } from "./toast";

const EMPTY: OauthConfig = { google_id: "", google_secret: "", microsoft_id: "" };

export function OauthAdvanced() {
  const [view, setView] = useState<OauthConfigView | null>(null);
  const [form, setForm] = useState<OauthConfig>(EMPTY);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.oauthConfigGet().then((v) => { setView(v); setForm({ google_id: v.google_id, google_secret: v.google_secret, microsoft_id: v.microsoft_id }); }).catch(() => setView(null));
  }, []);

  const dirty = view !== null && (form.google_id !== view.google_id || form.google_secret !== view.google_secret || form.microsoft_id !== view.microsoft_id);
  const set = (k: keyof OauthConfig, v: string) => setForm((f) => ({ ...f, [k]: v }));

  async function save() {
    setSaving(true);
    try {
      await api.oauthConfigSet(form);
      setView((v) => (v ? { ...v, ...form } : v));
      toast("Client IDs salvos", "success");
    } catch (e) {
      toast(`Não consegui salvar os Client IDs: ${e}`, "error");
    } finally {
      setSaving(false);
    }
  }

  return (
    <details className="oauth-advanced">
      <summary>Avançado: Client IDs do login com Google e Microsoft</summary>
      <p className="settings-sub">
        Crie os aplicativos conforme <code>docs/oauth-setup.md</code> e cole os IDs aqui. Valem mais que os embutidos na compilação (variáveis <code>POSTA_GOOGLE_CLIENT_ID</code>, <code>POSTA_GOOGLE_CLIENT_SECRET</code> e <code>POSTA_MS_CLIENT_ID</code>).
        {view?.built_in_google && " Esta versão já traz o ID do Google."}
        {view?.built_in_microsoft && " Esta versão já traz o ID da Microsoft."}
      </p>
      <label className="field">
        <span>Google: ID do cliente</span>
        <input value={form.google_id} onChange={(e) => set("google_id", e.target.value)} placeholder="123456-abc.apps.googleusercontent.com" autoComplete="off" spellCheck={false} />
      </label>
      <label className="field">
        <span>Google: segredo do cliente (se o Google fornecer)</span>
        <input type="password" value={form.google_secret} onChange={(e) => set("google_secret", e.target.value)} autoComplete="off" spellCheck={false} />
      </label>
      <label className="field">
        <span>Microsoft: ID do aplicativo (cliente)</span>
        <input value={form.microsoft_id} onChange={(e) => set("microsoft_id", e.target.value)} placeholder="00000000-0000-0000-0000-000000000000" autoComplete="off" spellCheck={false} />
      </label>
      <div>
        <button type="button" className="btn" disabled={!dirty || saving} onClick={save}>{saving ? "Salvando…" : "Salvar IDs"}</button>
      </div>
    </details>
  );
}
