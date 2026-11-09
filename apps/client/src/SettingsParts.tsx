// Peças comuns das páginas de Configurações.

/** Grupo de opções exclusivas; valor ausente ou desconhecido cai no `fallback`. */
export function Segmented({ label, value, fallback, options, onChange }: {
  label: string; value: string | undefined; fallback: string; options: [string, string][]; onChange: (v: string) => void;
}) {
  const current = options.some(([v]) => v === value) ? value : fallback;
  return (
    <div className="seg" role="radiogroup" aria-label={label}>
      {options.map(([v, text]) => (
        <button key={v} type="button" role="radio" aria-checked={v === current} onClick={() => onChange(v)}>{text}</button>
      ))}
    </div>
  );
}

export function Row({ title, sub, children }: { title: string; sub?: string; children: React.ReactNode }) {
  return (
    <div className="settings-row">
      <div>
        <div className="settings-strong">{title}</div>
        {sub && <div className="settings-sub">{sub}</div>}
      </div>
      {children}
    </div>
  );
}
