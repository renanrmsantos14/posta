// Preview ao vivo de Configurações > Aparência: um mini app (barra lateral, lista e leitor) com dados de exemplo.
// Reaproveita as classes do app dentro de um container com os próprios atributos (data-theme, data-radius...) e variáveis,
// então reflete as escolhas sem tocar no app de verdade. É só decorativo: aria-hidden e inert (sem foco nem clique).
import { CaretRight, FileText, PaperPlaneTilt, Star, Tray } from "@phosphor-icons/react";
import { ROW_H, type Density } from "./MessageList";
import { styleValue } from "./themes";

export interface PreviewProps {
  /** Tema base (`data-theme`) e variáveis do tema personalizado/destaque já resolvidas. */
  theme: string;
  vars: Record<string, string>;
  /** Preferências de estilo: size, density, railDensity, railAccounts, radius, shadow, font. */
  prefs: Record<string, string | undefined>;
}

const MESSAGES = [
  { from: "Equipe de Projetos", date: "11:42", subject: "Revisão do cronograma", snippet: "Segue a nova versão com os marcos da próxima semana.", unread: true, star: true, selected: true },
  { from: "Loja Central", date: "10:15", subject: "Seu pedido saiu para entrega", snippet: "A entrega está prevista para hoje, até as 18h.", unread: true },
  { from: "Carlos Souza", date: "9 de nov", subject: "Reunião de quinta-feira", snippet: "Podemos ajustar o horário para as 15h?" },
  { from: "Boletim Semanal", date: "7 de nov", subject: "Novidades da semana", snippet: "Três dicas para organizar a caixa de entrada." },
];

function Item({ icon: Icon, label, count, current, plain }: { icon: typeof Tray; label: string; count?: number; current?: boolean; plain?: boolean }) {
  return (
    <div className="nav-item" aria-current={current ? "page" : undefined}>
      <span className="nav-icon"><Icon size={16} weight="bold" /></span>
      <span className="nav-label">{label}</span>
      {count ? <span className={plain ? "count plain" : "count"}>{count}</span> : null}
    </div>
  );
}

function Account({ email, count, open, children }: { email: string; count: number; open: boolean; children: React.ReactNode }) {
  return (
    <section className="acct-sec" data-open={open || undefined}>
      <div className="acct-head">
        <CaretRight size={11} weight="bold" className="acct-caret" />
        <span className="acct-name">{email}</span>
        {count > 0 && <span className="count">{count}</span>}
      </div>
      <div className="acct-body"><div className="acct-inner">{children}</div></div>
    </section>
  );
}

export function AppearancePreview({ theme, vars, prefs }: PreviewProps) {
  const density: Density = prefs.density === "compact" || prefs.density === "comfortable" ? prefs.density : "medium";
  const size = ["s", "l"].includes(prefs.size ?? "") ? prefs.size : "m";
  const many = prefs.railAccounts !== "one";
  return (
    <div className="pv" aria-hidden inert data-theme={theme} data-size={size} data-radius={styleValue("radius", prefs.radius)}
      data-shadow={styleValue("shadow", prefs.shadow)} data-font={styleValue("font", prefs.font)} data-rail={styleValue("railDensity", prefs.railDensity)}
      style={vars}>
      <div className="pv-rail">
        <div className="acct-list">
          <Account email="ana@exemplo.com" count={12} open>
            <Item icon={Tray} label="Caixa de entrada" count={12} current />
            <Item icon={PaperPlaneTilt} label="Enviados" />
            <Item icon={FileText} label="Rascunhos" count={2} plain />
          </Account>
          <Account email="contato@exemplo.com" count={3} open={many}>
            <Item icon={Tray} label="Caixa de entrada" count={3} />
            <Item icon={PaperPlaneTilt} label="Enviados" />
          </Account>
        </div>
      </div>
      <div className="pv-main">
        <div className="list" data-density={density}>
          <div className="list-spacer">
            {MESSAGES.map((m) => (
              <div key={m.subject} className="row" aria-selected={m.selected || undefined} data-unread={m.unread || undefined} style={{ height: ROW_H[density] }}>
                <span className="dot" />
                <span className="from"><span className="from-name">{m.from}</span></span>
                <span className="date">{m.date}</span>
                <span className="star" data-on={m.star || undefined}><Star size={15} weight={m.star ? "fill" : "bold"} /></span>
                <span className="subject">{m.subject}</span>
                <span className="snippet">{m.snippet}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="pv-reader">
          <h4>Revisão do cronograma</h4>
          <div className="pv-meta">Equipe de Projetos · 11:42</div>
          <p>Olá, segue a nova versão do cronograma com os marcos revisados. Confira as datas e me avise se algo precisar de ajuste.</p>
          <span className="btn primary small">Responder</span>
        </div>
      </div>
    </div>
  );
}
