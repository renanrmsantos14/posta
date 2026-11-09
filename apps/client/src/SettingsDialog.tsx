// Configurações: notificações, assinatura por conta, contas e atualizações. Fecha por X, Esc ou clique fora.
import { useEffect, useState } from "react";
import { ArrowsClockwise, Trash, X } from "@phosphor-icons/react";
import { api, type Account, type Settings } from "./api";
import { autoCheck, checkUpdate, setAutoCheck, type Update } from "./updater";
import { toast } from "./toast";
import build from "./version.json";

interface Props {
  accounts: Account[];
  settings: Settings;
  onSetting: (key: string, value: string) => void;
  /** Resolve quando a conta foi removida; rejeita com a mensagem de erro. */
  onRemoveAccount: (id: string) => Promise<void>;
  onClose: () => void;
  onUpdate: (u: Update) => void;
}

const EXIT_MS = 150;

export function SettingsDialog({ accounts, settings, onSetting, onRemoveAccount, onClose, onUpdate }: Props) {
  const [auto, setAuto] = useState(autoCheck);
  const [checking, setChecking] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [sigAccount, setSigAccount] = useState(accounts[0]?.id ?? "");
  const [removing, setRemoving] = useState<Account | null>(null);
  const [removeBusy, setRemoveBusy] = useState(false);
  const notify = settings.notify !== "0";
  const sigId = accounts.some((a) => a.id === sigAccount) ? sigAccount : accounts[0]?.id ?? "";

  async function confirmRemove() {
    if (!removing || removeBusy) return;
    setRemoveBusy(true);
    try {
      await onRemoveAccount(removing.id);
    } catch (e) {
      toast(`Não consegui remover a conta: ${e}`, "error");
    } finally {
      setRemoveBusy(false);
      setRemoving(null);
    }
  }

  async function toggleNotify(on: boolean) {
    onSetting("notify", on ? "1" : "0");
    if (!on) return;
    try {
      if (!(await api.requestNotifyPermission())) toast("O sistema bloqueou as notificações do Posta", "error");
    } catch (e) {
      toast(`Não consegui pedir permissão de notificações: ${e}`, "error");
    }
  }

  function close(then = onClose) {
    setLeaving(true);
    setTimeout(then, EXIT_MS);
  }

  async function check() {
    setChecking(true);
    try {
      const found = await checkUpdate();
      if (found) close(() => onUpdate(found));
      else toast("Você está na versão mais recente", "success");
    } catch (e) {
      toast(`Não consegui procurar atualizações: ${e}`, "error");
    } finally {
      setChecking(false);
    }
  }

  function toggle(on: boolean) {
    setAuto(on);
    setAutoCheck(on);
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      if (removing) { if (!removeBusy) setRemoving(null); }
      else close();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && close()}>
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="set-title">
        <header className="dialog-head">
          <h2 id="set-title">Configurações</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={() => close()}>
            <X size={16} weight="bold" />
          </button>
        </header>

        <section className="settings-group" aria-labelledby="set-notify">
          <h3 id="set-notify" className="rail-label">Notificações</h3>
          <label className="settings-row switch-row">
            <div>
              <div className="settings-strong">Avisar de email novo</div>
              <div className="settings-sub">Notificação do sistema com remetente e assunto, quando a janela não está em foco</div>
            </div>
            <input type="checkbox" role="switch" className="switch" checked={notify} onChange={(e) => toggleNotify(e.target.checked)} />
          </label>
        </section>

        <section className="settings-group" aria-labelledby="set-undo">
          <h3 id="set-undo" className="rail-label">Envio</h3>
          <label className="settings-row">
            <div>
              <div className="settings-strong">Desfazer envio</div>
              <div className="settings-sub">Tempo para desfazer depois de tocar em Enviar</div>
            </div>
            <select aria-label="Tempo para desfazer o envio" value={settings.undo_send ?? "5"} onChange={(e) => onSetting("undo_send", e.target.value)}>
              <option value="0">Desligado</option>
              <option value="5">5 segundos</option>
              <option value="10">10 segundos</option>
            </select>
          </label>
        </section>

        {accounts.length > 0 && (
          <section className="settings-group" aria-labelledby="set-sig">
            <h3 id="set-sig" className="rail-label">Assinatura</h3>
            <div className="settings-col">
              {accounts.length > 1 && (
                <select aria-label="Conta da assinatura" value={sigId} onChange={(e) => setSigAccount(e.target.value)}>
                  {accounts.map((a) => <option key={a.id} value={a.id}>{a.username}</option>)}
                </select>
              )}
              <textarea className="signature-input" rows={4} aria-label="Assinatura" placeholder="Entra no fim das mensagens novas e das respostas"
                value={settings[`signature:${sigId}`] ?? ""} onChange={(e) => onSetting(`signature:${sigId}`, e.target.value)} />
            </div>
          </section>
        )}

        {accounts.length > 0 && (
          <section className="settings-group" aria-labelledby="set-accounts">
            <h3 id="set-accounts" className="rail-label">Contas</h3>
            {accounts.map((a) => (
              <div key={a.id} className="settings-row">
                <div className="settings-account">
                  <div className="settings-strong">{a.username}</div>
                  <div className="settings-sub">{a.host}</div>
                </div>
                <button type="button" className="btn" onClick={() => setRemoving(a)}>
                  <Trash size={15} weight="bold" /> Remover
                </button>
              </div>
            ))}
          </section>
        )}

        <section className="settings-group" aria-labelledby="set-updates">
          <h3 id="set-updates" className="rail-label">Atualizações</h3>
          <div className="settings-row">
            <div>
              <div className="settings-strong">Posta v{build.version}</div>
              <div className="settings-sub">Compilado em {build.builtAt}</div>
            </div>
            <button type="button" className="btn" onClick={check} disabled={checking} data-busy={checking || undefined}>
              <ArrowsClockwise size={15} weight="bold" className="sync-icon" />
              {checking ? "Procurando…" : "Procurar atualizações"}
            </button>
          </div>
          <label className="settings-row switch-row">
            <div>
              <div className="settings-strong">Procurar automaticamente</div>
              <div className="settings-sub">Ao abrir o app e a cada 6 horas</div>
            </div>
            <input type="checkbox" role="switch" className="switch" checked={auto} onChange={(e) => toggle(e.target.checked)} />
          </label>
        </section>
      </div>
      {removing && (
        <div className="confirm scrim" onMouseDown={(e) => { if (e.target === e.currentTarget && !removeBusy) setRemoving(null); }}>
          <div className="confirm-card" role="alertdialog" aria-labelledby="remove-title">
            <p id="remove-title">Remover conta?</p>
            <span>{removing.username} sai do Posta: a senha, as mensagens e os rascunhos desta conta apagam deste aparelho. Os emails continuam no servidor.</span>
            <div className="confirm-actions">
              <button type="button" className="btn" onClick={() => setRemoving(null)} disabled={removeBusy} autoFocus>Cancelar</button>
              <button type="button" className="btn danger" onClick={confirmRemove} disabled={removeBusy}>Remover</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
