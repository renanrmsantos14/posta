// Configurações de produtividade: regras automáticas, respostas prontas e resposta de férias.
import { useEffect, useState } from "react";
import { Play, Plus, X } from "@phosphor-icons/react";
import { api, type Settings } from "./api";
import { parseCategories } from "./Categories";
import { ACTION_LABEL, FIELD_LABEL, parseRules, parseTemplates, parseVacation, type Rule, type Template } from "./productivity";

interface Props {
  settings: Settings;
  onSetting: (key: string, value: string) => void;
}

const MAX_TEMPLATES = 20;

export function RulesSection({ settings, onSetting, onApply }: Props & { onApply: () => Promise<void> }) {
  const rules = parseRules(settings.rules);
  const categories = parseCategories(settings.categories);
  const [folders, setFolders] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.userFolders().then((l) => setFolders([...new Set(l.map((f) => f.name))])).catch(() => {});
  }, []);

  const save = (list: Rule[]) => onSetting("rules", JSON.stringify(list));
  const patch = (id: string, change: Partial<Rule>) => save(rules.map((r) => (r.id === id ? { ...r, ...change } : r)));
  const add = () => save([...rules, { id: `r${Date.now().toString(36)}`, field: "from", contains: "", action: "read", arg: "", enabled: true }]);
  /** Ao trocar a ação, o argumento vira o primeiro valor possível (ou vazio). */
  const setAction = (r: Rule, action: Rule["action"]) =>
    patch(r.id, { action, arg: action === "move" ? folders[0] ?? "" : action === "category" ? categories[0]?.id ?? "" : "" });
  const apply = async () => { setBusy(true); try { await onApply(); } finally { setBusy(false); } };

  return (
    <section className="settings-group" aria-label="Regras">
      <p className="settings-sub">Valem para as mensagens novas da Entrada. Se várias regras casam, as ações se somam; para mover e para categoria vale a primeira da lista. Maiúsculas e acentos não importam.</p>
      {rules.map((r) => {
        const incomplete = !r.contains.trim() || (r.action === "move" && !r.arg) || (r.action === "category" && !r.arg);
        return (
          <div key={r.id} className="settings-row rule-row" data-off={!r.enabled || undefined}>
            <input type="checkbox" aria-label="Regra ativa" checked={r.enabled} onChange={(e) => patch(r.id, { enabled: e.target.checked })} />
            <span className="rule-if">Se</span>
            <select aria-label="Campo" value={r.field} onChange={(e) => patch(r.id, { field: e.target.value as Rule["field"] })}>
              {(Object.keys(FIELD_LABEL) as Rule["field"][]).map((f) => <option key={f} value={f}>{FIELD_LABEL[f]}</option>)}
            </select>
            <span className="rule-if">contém</span>
            <input className="rule-text" aria-label="Texto" placeholder="texto" value={r.contains} onChange={(e) => patch(r.id, { contains: e.target.value })} />
            <span className="rule-if">então</span>
            <select aria-label="Ação" value={r.action} onChange={(e) => setAction(r, e.target.value as Rule["action"])}>
              {(Object.keys(ACTION_LABEL) as Rule["action"][]).map((a) => <option key={a} value={a}>{ACTION_LABEL[a]}</option>)}
            </select>
            {r.action === "move" && (
              <select aria-label="Pasta" value={r.arg} onChange={(e) => patch(r.id, { arg: e.target.value })}>
                {!folders.includes(r.arg) && <option value={r.arg}>{r.arg || "Escolha a pasta"}</option>}
                {folders.map((f) => <option key={f} value={f}>{f}</option>)}
              </select>
            )}
            {r.action === "category" && (
              <select aria-label="Categoria" value={r.arg} onChange={(e) => patch(r.id, { arg: e.target.value })}>
                {!categories.some((c) => c.id === r.arg) && <option value={r.arg}>Escolha a categoria</option>}
                {categories.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </select>
            )}
            {incomplete && <span className="rule-warn" role="status">Incompleta: não será aplicada</span>}
            <button type="button" className="icon-btn" aria-label="Remover regra" onClick={() => save(rules.filter((x) => x.id !== r.id))}>
              <X size={14} weight="bold" />
            </button>
          </div>
        );
      })}
      {rules.length === 0 && <p className="settings-sub">Nenhuma regra ainda.</p>}
      <div className="settings-row rule-actions">
        <button type="button" className="btn" onClick={add}><Plus size={15} weight="bold" /> Adicionar regra</button>
        <button type="button" className="btn" onClick={apply} disabled={busy || !rules.some((r) => r.enabled)}>
          <Play size={15} weight="bold" /> {busy ? "Aplicando…" : "Aplicar agora à Entrada"}
        </button>
      </div>
    </section>
  );
}

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
        Responde sozinha, uma vez por remetente no período, enquanto o Posta estiver aberto. Não responde a listas, endereços “não responda” nem mensagens automáticas.
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
