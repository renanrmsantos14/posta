// Configurações de produtividade: respostas prontas e resposta de férias (as regras ficam em Rules.tsx).
import { Plus, X } from "@phosphor-icons/react";
import type { Settings } from "./api";
import { parseTemplates, parseVacation, type Template } from "./productivity";

export { RulesSection } from "./Rules";

interface Props {
  settings: Settings;
  onSetting: (key: string, value: string) => void;
}

const MAX_TEMPLATES = 20;

export function TemplatesSection({ settings, onSetting }: Props) {
  const list = parseTemplates(settings.templates);
  const save = (next: Template[]) => onSetting("templates", JSON.stringify(next));
  const patch = (id: string, change: Partial<Template>) => save(list.map((t) => (t.id === id ? { ...t, ...change } : t)));
  return (
    <section className="settings-group" aria-label="Respostas prontas">
      <p className="settings-sub">Textos que você insere no Composer pelo botão “Modelos”. Ficam só neste aparelho.</p>
      {list.map((t) => (
        <div key={t.id} className="settings-col tpl-card">
          <div className="tpl-head">
            <input className="cat-name" aria-label="Nome do modelo" placeholder="Nome" maxLength={40} value={t.name} onChange={(e) => patch(t.id, { name: e.target.value })} />
            <button type="button" className="icon-btn" aria-label={`Remover ${t.name || "modelo"}`} onClick={() => save(list.filter((x) => x.id !== t.id))}>
              <X size={14} weight="bold" />
            </button>
          </div>
          <textarea className="signature-input" rows={4} aria-label="Texto do modelo" placeholder="Texto da resposta" value={t.body} onChange={(e) => patch(t.id, { body: e.target.value })} />
        </div>
      ))}
      <div className="settings-row">
        <button type="button" className="btn" disabled={list.length >= MAX_TEMPLATES}
          onClick={() => save([...list, { id: `t${Date.now().toString(36)}`, name: "Novo modelo", body: "" }])}>
          <Plus size={15} weight="bold" /> Adicionar modelo
        </button>
      </div>
    </section>
  );
}

export function VacationSection({ settings, onSetting }: Props) {
  const v = parseVacation(settings.vacation);
  const set = (change: Partial<typeof v>) => onSetting("vacation", JSON.stringify({ ...v, ...change }));
  return (
    <section className="settings-group" aria-label="Férias">
      <p className="settings-sub">
        Responde sozinha, uma vez por remetente no período, enquanto o RenanMail estiver aberto. Não responde a listas, endereços “não responda” nem mensagens automáticas.
      </p>
      <label className="settings-row vacation-on">
        <span className="settings-strong">Resposta automática ligada</span>
        <input type="checkbox" role="switch" aria-label="Resposta automática ligada" checked={v.enabled} onChange={(e) => set({ enabled: e.target.checked })} />
      </label>
      <div className="settings-col">
        <div className="field-row">
          <label className="field">Início<input type="date" value={v.start} max={v.end || undefined} onChange={(e) => set({ start: e.target.value })} /></label>
          <label className="field">Fim (dia inteiro)<input type="date" value={v.end} min={v.start || undefined} onChange={(e) => set({ end: e.target.value })} /></label>
        </div>
        <label className="field">Assunto
          <input type="text" placeholder="Fora do escritório" maxLength={120} value={v.subject} onChange={(e) => set({ subject: e.target.value })} />
        </label>
        <label className="field">Mensagem
          <textarea className="signature-input" rows={5} placeholder="Estou fora até dia 20 e respondo na volta." value={v.body} onChange={(e) => set({ body: e.target.value })} />
        </label>
        {v.enabled && !v.body.trim() && <p className="form-error" role="alert">Escreva a mensagem: sem texto a resposta não é enviada.</p>}
        {v.start && v.end && v.end < v.start && <p className="form-error" role="alert">O fim é anterior ao início.</p>}
      </div>
    </section>
  );
}
