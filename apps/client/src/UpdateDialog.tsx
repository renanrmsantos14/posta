// Versão nova disponível: mostra as novidades, baixa com progresso e reinicia.
// Fecha por "Depois", X, Esc ou clique fora; durante o download não fecha.
import { useEffect, useState } from "react";
import { ArrowClockwise, DownloadSimple, X } from "@phosphor-icons/react";
import { restart, type Update } from "./updater";
import build from "./version.json";

interface Props { update: Update; onLater: () => void }

type Status = "idle" | "downloading" | "restarting" | "error";
const EXIT_MS = 150;

export function UpdateDialog({ update, onLater }: Props) {
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState<number | null>(0);
  const [error, setError] = useState("");
  const [leaving, setLeaving] = useState(false);
  const busy = status === "downloading" || status === "restarting";

  function requestClose() {
    if (busy) return;
    setLeaving(true);
    setTimeout(onLater, EXIT_MS);
  }

  async function install() {
    setStatus("downloading");
    setError("");
    try {
      await update.install(setProgress);
      setStatus("restarting");
      await restart();
    } catch (e) {
      setStatus("error");
      setError(`Não consegui instalar: ${e}`);
    }
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      requestClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  const notes = update.notes.split("\n").map((l) => l.replace(/^\s*[-*•]\s*/, "").trim()).filter(Boolean);
  const pct = progress === null ? null : Math.round(progress * 100);

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <div className="dialog update" role="dialog" aria-modal="true" aria-labelledby="upd-title">
        <header className="dialog-head">
          <h2 id="upd-title">Nova versão do Posta</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={busy}>
            <X size={16} weight="bold" />
          </button>
        </header>
        <p className="update-versions">
          <span>v{build.version}</span> <span aria-hidden>→</span> <strong>v{update.version}</strong>
        </p>
        {notes.length > 0 && (
          <ul className="update-notes" aria-label="Novidades">
            {notes.map((n, i) => <li key={i} style={{ "--i": i } as React.CSSProperties}>{n}</li>)}
          </ul>
        )}

        {busy && (
          <div className="update-progress" role="progressbar" aria-label="Baixando atualização"
            aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct ?? undefined}>
            <div className="bar" data-indeterminate={pct === null || undefined}>
              <span style={{ transform: `scaleX(${status === "restarting" ? 1 : (progress ?? 0)})` }} />
            </div>
            <span className="update-status">
              {status === "restarting" ? "Reiniciando…" : pct === null ? "Baixando…" : `Baixando… ${pct}%`}
            </span>
          </div>
        )}
        {error && <p className="form-error" role="alert">{error}</p>}

        {!busy && (
          <div className="confirm-actions">
            <button type="button" className="btn primary" onClick={install} autoFocus>
              {status === "error" ? <ArrowClockwise size={15} weight="bold" /> : <DownloadSimple size={15} weight="bold" />}
              {status === "error" ? "Tentar de novo" : "Atualizar agora"}
            </button>
            <button type="button" className="btn" onClick={requestClose}>Depois</button>
          </div>
        )}
      </div>
    </div>
  );
}
