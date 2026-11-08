// Configurações: versão instalada e atualizações. Fecha por X, Esc ou clique fora.
import { useEffect, useState } from "react";
import { ArrowsClockwise, X } from "@phosphor-icons/react";
import { autoCheck, canUpdate, checkUpdate, setAutoCheck, type Update } from "./updater";
import { toast } from "./toast";
import build from "./version.json";

interface Props { onClose: () => void; onUpdate: (u: Update) => void }

const EXIT_MS = 150;

export function SettingsDialog({ onClose, onUpdate }: Props) {
  const [auto, setAuto] = useState(autoCheck);
  const [checking, setChecking] = useState(false);
  const [leaving, setLeaving] = useState(false);

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
      close();
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

        <section className="settings-group" aria-labelledby="set-updates">
          <h3 id="set-updates" className="rail-label">Atualizações</h3>
          <div className="settings-row">
            <div>
              <div className="settings-strong">Posta v{build.version}</div>
              <div className="settings-sub">Compilado em {build.builtAt}</div>
            </div>
            {canUpdate && <button type="button" className="btn" onClick={check} disabled={checking} data-busy={checking || undefined}>
              <ArrowsClockwise size={15} weight="bold" className="sync-icon" />
              {checking ? "Procurando…" : "Procurar atualizações"}
            </button>}
          </div>
          {canUpdate && <label className="settings-row switch-row">
            <div>
              <div className="settings-strong">Procurar automaticamente</div>
              <div className="settings-sub">Ao abrir o app e a cada 6 horas</div>
            </div>
            <input type="checkbox" role="switch" className="switch" checked={auto} onChange={(e) => toggle(e.target.checked)} />
          </label>}
        </section>
      </div>
    </div>
  );
}
