// Regras de triagem: lista ordenável com avisos e editor em painel (várias condições e ações, prévia de quantas mensagens pega).
import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, ArrowUp, PencilSimple, Play, Plus, Trash, X } from "@phosphor-icons/react";
import { api, type Settings } from "./api";
import { parseCategories } from "./Categories";
import {
  ACTION_LABEL, ATTACHMENT_LABEL, FIELD_LABEL, describeRule, newRule, parseRules, ruleIssues, ruleReady,
  type ActionKind, type CondField, type Rule,
} from "./productivity";

interface Props {
  settings: Settings;
  onSetting: (key: string, value: string) => void;
  /** Aplica às mensagens que já estão na Entrada (só a regra `only`, se informada). */
  onApply: (only?: string) => Promise<void>;
}

const EXIT_MS = 150;
const FIELDS = Object.keys(FIELD_LABEL) as CondField[];
const ACTIONS = Object.keys(ACTION_LABEL) as ActionKind[];

export function RulesSection({ settings, onSetting, onApply }: Props) {
  const rules = parseRules(settings.rules);
  const categories = parseCategories(settings.categories);
  const [folders, setFolders] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [editing, setEditing] = useState<Rule | null>(null);
  useEffect(() => {
    api.userFolders().then((l) => setFolders([...new Set(l.map((f) => f.name))])).catch(() => {});
  }, []);

  const save = (list: Rule[]) => onSetting("rules", JSON.stringify(list));
  const issues = useMemo(() => ruleIssues(rules, { folders, categories: categories.map((c) => c.id) }), [rules, folders, categories]);
  const catName = (id: string) => categories.find((c) => c.id === id)?.name ?? "removida";
  const move = (i: number, to: number) => {
    if (to < 0 || to >= rules.length) return;
    const next = [...rules];
    [next[i], next[to]] = [next[to], next[i]];
    save(next);
  };
  const apply = async (only?: string) => { setBusy(true); try { await onApply(only); } finally { setBusy(false); } };

  return (
    <section className="settings-group" aria-label="Regras">
      <p className="settings-sub">
        Valem para as mensagens novas da Entrada e, se você pedir, para as que já estão lá. A ordem da lista importa: as regras rodam de cima para baixo, as ações se somam,
        e para mover e para categoria vale a primeira. Lixeira vence mover. Maiúsculas e acentos não importam.
      </p>
      {rules.map((r, i) => {
        const problems = issues.get(r.id) ?? [];
        return (
          <div key={r.id} className="settings-row rule-card" data-off={!r.enabled || undefined}>
            <input type="checkbox" role="switch" aria-label={`Regra ${i + 1} ativa`} checked={r.enabled} onChange={(e) => save(rules.map((x) => (x.id === r.id ? { ...x, enabled: e.target.checked } : x)))} />
            <div className="rule-body">
              <strong>{r.name.trim() || `Regra ${i + 1}`}</strong>
              <span className="rule-desc">{describeRule(r, catName)}</span>
              {problems.map((p) => <span key={p} className="rule-warn" role="status">{p}</span>)}
            </div>
            <div className="rule-tools">
              <button type="button" className="icon-btn" aria-label="Subir regra" title="Subir" disabled={i === 0} onClick={() => move(i, i - 1)}><ArrowUp size={14} weight="bold" /></button>
              <button type="button" className="icon-btn" aria-label="Descer regra" title="Descer" disabled={i === rules.length - 1} onClick={() => move(i, i + 1)}><ArrowDown size={14} weight="bold" /></button>
              <button type="button" className="icon-btn" aria-label="Editar regra" title="Editar" onClick={() => setEditing(r)}><PencilSimple size={14} weight="bold" /></button>
              <button type="button" className="icon-btn" aria-label="Remover regra" title="Remover" onClick={() => save(rules.filter((x) => x.id !== r.id))}><Trash size={14} weight="bold" /></button>
            </div>
          </div>
        );
      })}
      {rules.length === 0 && <p className="settings-sub">Nenhuma regra ainda.</p>}
      <div className="settings-row rule-actions">
        <button type="button" className="btn" onClick={() => setEditing(newRule())}><Plus size={15} weight="bold" /> Nova regra</button>
        <button type="button" className="btn" onClick={() => void apply()} disabled={busy || !rules.some((r) => r.enabled && ruleReady(r))}>
          <Play size={15} weight="bold" /> {busy ? "Aplicando…" : "Aplicar todas à Entrada"}
        </button>
      </div>
      {editing && (
        <RuleDialog
          rule={editing}
          isNew={!rules.some((r) => r.id === editing.id)}
          folders={folders}
          categories={categories}
          onSave={async (rule, applyNow) => {
            save(rules.some((r) => r.id === rule.id) ? rules.map((r) => (r.id === rule.id ? rule : r)) : [...rules, rule]);
            if (applyNow) await onApply(rule.id);
          }}
          onClose={() => setEditing(null)}
        />
      )}
    </section>
  );
}

interface DialogProps {
  rule: Rule;
  isNew: boolean;
  folders: string[];
  categories: { id: string; name: string }[];
  onSave: (rule: Rule, applyNow: boolean) => Promise<void>;
  onClose: () => void;
}

/** Cria ou edita uma regra. Fecha por X, Esc ou clique fora; com alteração pergunta "Descartar alterações?". */
function RuleDialog({ rule, isNew, folders, categories, onSave, onClose }: DialogProps) {
  const [draft, setDraft] = useState<Rule>(rule);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [matches, setMatches] = useState<number | null>(null);
  const first = useRef<HTMLInputElement>(null);
  const initial = useRef(JSON.stringify(rule)).current;
  const dirty = JSON.stringify(draft) !== initial;
  const ready = ruleReady(draft);

  useEffect(() => first.current?.focus(), []);

  // Prévia: quantas mensagens da Entrada a regra pegaria agora (espera 300 ms depois da última digitação).
  useEffect(() => {
    if (!ready) { setMatches(null); return; }
    let live = true;
    const t = setTimeout(() => { api.countRuleMatches(draft).then((n) => live && setMatches(n)).catch(() => live && setMatches(null)); }, 300);
    return () => { live = false; clearTimeout(t); };
  }, [draft, ready]);

  function close() {
    setLeaving(true);
    setTimeout(onClose, EXIT_MS);
  }
  function requestClose() {
    if (busy) return;
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

  const patch = (change: Partial<Rule>) => setDraft((d) => ({ ...d, ...change }));
  const setCond = (i: number, change: Partial<Rule["conditions"][number]>) => patch({ conditions: draft.conditions.map((c, k) => (k === i ? { ...c, ...change } : c)) });
  const setAct = (i: number, change: Partial<Rule["actions"][number]>) => patch({ actions: draft.actions.map((a, k) => (k === i ? { ...a, ...change } : a)) });
  const defaultArg = (kind: ActionKind) => (kind === "move" ? folders[0] ?? "" : kind === "category" ? categories[0]?.id ?? "" : "");

  async function submit(applyNow: boolean) {
    if (!ready || busy) return;
    setBusy(true);
    try {
      await onSave(draft, applyNow);
      close();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="overlay" data-leaving={leaving || undefined} onPointerDown={(e) => e.target === e.currentTarget && requestClose()}>
      <form className="dialog rule-dialog" role="dialog" aria-modal="true" aria-labelledby="rule-title" onSubmit={(e) => { e.preventDefault(); void submit(false); }}>
        <header className="dialog-head">
          <h2 id="rule-title">{isNew ? "Nova regra" : "Editar regra"}</h2>
          <button type="button" className="icon-btn" aria-label="Fechar" onClick={requestClose} disabled={busy}><X size={16} weight="bold" /></button>
        </header>

        <label className="field">
          <span>Nome (opcional)</span>
          <input ref={first} maxLength={60} value={draft.name} placeholder="Faturas dos bancos" onChange={(e) => patch({ name: e.target.value })} />
        </label>

        <fieldset className="rule-group">
          <legend>Quando a mensagem tiver <small>(todas as condições)</small></legend>
          {draft.conditions.map((c, i) => (
            <div key={i} className="rule-line">
              <select aria-label="Campo" value={c.field} onChange={(e) => setCond(i, { field: e.target.value as CondField, value: e.target.value === "attachment" ? "yes" : c.field === "attachment" ? "" : c.value })}>
                {FIELDS.map((f) => <option key={f} value={f}>{FIELD_LABEL[f]}</option>)}
              </select>
              {c.field === "attachment" ? (
                <select aria-label="Anexo" value={c.value === "no" ? "no" : "yes"} onChange={(e) => setCond(i, { value: e.target.value })}>
                  <option value="yes">{ATTACHMENT_LABEL.yes}</option>
                  <option value="no">{ATTACHMENT_LABEL.no}</option>
                </select>
              ) : (
                <input aria-label="Texto" className="rule-text" value={c.value} placeholder={c.field === "domain" ? "itau.com.br" : "texto"} onChange={(e) => setCond(i, { value: e.target.value })} />
              )}
              <button type="button" className="icon-btn" aria-label="Remover condição" disabled={draft.conditions.length === 1} onClick={() => patch({ conditions: draft.conditions.filter((_, k) => k !== i) })}><X size={14} weight="bold" /></button>
            </div>
          ))}
          <button type="button" className="btn small" onClick={() => patch({ conditions: [...draft.conditions, { field: "subject", value: "" }] })}><Plus size={14} weight="bold" /> Condição</button>
        </fieldset>

        <fieldset className="rule-group">
          <legend>Então</legend>
          {draft.actions.map((a, i) => (
            <div key={i} className="rule-line">
              <select aria-label="Ação" value={a.kind} onChange={(e) => setAct(i, { kind: e.target.value as ActionKind, arg: defaultArg(e.target.value as ActionKind) })}>
                {ACTIONS.map((k) => <option key={k} value={k}>{ACTION_LABEL[k]}</option>)}
              </select>
              {a.kind === "move" && (
                <select aria-label="Pasta" value={a.arg} onChange={(e) => setAct(i, { arg: e.target.value })}>
                  {!folders.includes(a.arg) && <option value={a.arg}>{a.arg ? `${a.arg} (não existe)` : "Escolha a pasta"}</option>}
                  {folders.map((f) => <option key={f} value={f}>{f}</option>)}
                </select>
              )}
              {a.kind === "category" && (
                <select aria-label="Categoria" value={a.arg} onChange={(e) => setAct(i, { arg: e.target.value })}>
                  {!categories.some((c) => c.id === a.arg) && <option value={a.arg}>Escolha a categoria</option>}
                  {categories.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                </select>
              )}
              <button type="button" className="icon-btn" aria-label="Remover ação" disabled={draft.actions.length === 1} onClick={() => patch({ actions: draft.actions.filter((_, k) => k !== i) })}><X size={14} weight="bold" /></button>
            </div>
          ))}
          <button type="button" className="btn small" onClick={() => patch({ actions: [...draft.actions, { kind: "star", arg: "" }] })}><Plus size={14} weight="bold" /> Ação</button>
        </fieldset>

        <label className="rule-stop">
          <input type="checkbox" checked={draft.stop} onChange={(e) => patch({ stop: e.target.checked })} />
          <span>Se esta regra casar, não aplicar as regras seguintes</span>
        </label>

        <p className="rule-preview" role="status" aria-live="polite">
          {!ready ? "Preencha as condições e as ações para ver quantas mensagens ela pega." : matches === null ? "Contando…" : matches === 0 ? "Nenhuma mensagem da Entrada casa com esta regra agora." : `${matches} ${matches === 1 ? "mensagem" : "mensagens"} da Entrada ${matches === 1 ? "casa" : "casam"} com esta regra.`}
        </p>

        <div className="dialog-actions">
          <button type="submit" className="btn primary" disabled={!ready || busy}>{busy ? "Salvando…" : "Salvar"}</button>
          <button type="button" className="btn" disabled={!ready || busy} onClick={() => void submit(true)}>Salvar e aplicar às existentes</button>
        </div>

        {confirming && (
          <div className="confirm" role="alertdialog" aria-labelledby="rule-confirm">
            <p id="rule-confirm">Descartar alterações?</p>
            <div className="confirm-actions">
              <button type="button" className="btn" onClick={() => setConfirming(false)} autoFocus>Continuar editando</button>
              <button type="button" className="btn danger" onClick={close}>Descartar</button>
            </div>
          </div>
        )}
      </form>
    </div>
  );
}
