// Configurações > Aparência: tema (embutidos e personalizados), barra lateral, lista e estilo, com preview ao vivo ao lado.
import { useEffect, useMemo, useRef, useState } from "react";
import { Check } from "@phosphor-icons/react";
import type { Settings } from "./api";
import { AppearancePreview } from "./AppearancePreview";
import { Row, Segmented } from "./SettingsParts";
import {
  ACCENTS, HEX, THEMES, TOKENS, accentOver, contrastRatio, customVars, fitAccent, minContrast, newCustomId, parseCustomThemes, readColors, resolveTheme,
  themePref, themeStyle, type Colors, type CustomTheme, type TokenKey,
} from "./themes";
import { TOOLBAR_ITEMS, hiddenTools } from "./commands";
import { toast } from "./toast";

interface Props {
  settings: Settings;
  onSetting: (key: string, value: string) => void;
  onPreview: (key: string, value: string) => void;
  accentAdjusted: boolean;
  /** Há tema personalizado ou renomeação em edição (a tela de Configurações confirma antes de sair). */
  onDirty: (dirty: boolean) => void;
}

/** Mini janela: trilho, lista (não lida, selecionada, lida) e leitor, só com os tokens do tema do elemento pai. */
function Mini() {
  return (
    <span className="mini" aria-hidden>
      <span className="mini-rail"><i /><i /><i /></span>
      <span className="mini-list">
        <b className="unread"><u /><i /><i /></b>
        <b className="sel"><i /><i /></b>
        <b><i /><i /></b>
      </span>
      <span className="mini-read"><i className="title" /><i /><i /><i className="short" /><em /></span>
    </span>
  );
}

/** Tema personalizado em edição; `id` nulo = ainda não salvo. */
interface Draft { id: string | null; name: string; base: string; colors: Colors }
const MAX_CUSTOM = 12;
const LABEL = Object.fromEntries(TOKENS.map(([k, , label]) => [k, label])) as Record<TokenKey, string>;
/** Pares de texto sobre fundo que pedem 4,5:1. */
const TEXT_PAIRS: [TokenKey, TokenKey][] = [["text", "surface"], ["strong", "surface"], ["muted", "surface"]];

export function AppearanceSection({ settings, onSetting, onPreview, accentAdjusted, onDirty }: Props) {
  const customs = useMemo(() => parseCustomThemes(settings.customThemes), [settings.customThemes]);
  const pref = themePref(settings.theme, customs);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  useEffect(() => onDirty(!!draft || !!renaming), [draft, renaming, onDirty]);
  useEffect(() => () => onDirty(false), [onDirty]);
  const group = useRef<HTMLDivElement>(null);
  const accent = HEX.test(settings.accent ?? "") ? settings.accent!.toLowerCase() : "";

  function pick(id: string, name: string) {
    if (id === pref) return;
    onSetting("theme", id);
    toast(id === "system" ? "Tema segue o sistema" : `Tema ${name} aplicado`, "success");
  }

  /** Setas movem e escolhem, como num grupo de rádio. */
  function onKey(e: React.KeyboardEvent) {
    const keys = ["ArrowRight", "ArrowDown", "ArrowLeft", "ArrowUp", "Home", "End"];
    if (!keys.includes(e.key)) return;
    e.preventDefault();
    const cards = [...(group.current?.querySelectorAll<HTMLButtonElement>("[role=radio]") ?? [])];
    const at = cards.findIndex((c) => c === document.activeElement);
    const next = e.key === "Home" ? 0 : e.key === "End" ? cards.length - 1
      : (at + (e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : -1) + cards.length) % cards.length;
    cards[next]?.focus();
    cards[next]?.click();
  }

  // Cor do seletor: confirma ~0,4 s depois do último movimento (a tela já mostra o resultado enquanto arrasta).
  const timer = useRef<number | undefined>(undefined);
  const pending = useRef<string | null>(null);
  const commit = useRef(onSetting);
  commit.current = onSetting;
  const flush = () => {
    window.clearTimeout(timer.current);
    if (pending.current !== null) commit.current("accent", pending.current);
    pending.current = null;
  };
  useEffect(() => flush, []); // sair da página antes dos 0,4 s não perde a cor
  function dragAccent(value: string) {
    onPreview("accent", value);
    pending.current = value;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, 400);
  }
  function setAccent(value: string) {
    pending.current = null;
    window.clearTimeout(timer.current);
    onSetting("accent", value);
    toast(value ? "Cor de destaque aplicada" : "Cor de destaque do tema restaurada", "success");
  }

  const saveCustoms = (list: CustomTheme[]) => onSetting("customThemes", JSON.stringify(list));

  /** Parte do tema atual (embutido ou personalizado). */
  function startDraft() {
    const cur = customs.find((c) => c.id === pref);
    const base = cur ? cur.base : resolveTheme(pref);
    setDraft({ id: null, name: `Meu tema ${customs.length + 1}`, base, colors: cur ? { ...cur.colors } : readColors(base) });
  }
  function saveDraft() {
    if (!draft) return;
    const name = draft.name.trim() || "Meu tema";
    const theme: CustomTheme = { id: draft.id ?? newCustomId(), name, base: draft.base, colors: draft.colors };
    saveCustoms(draft.id ? customs.map((c) => (c.id === theme.id ? theme : c)) : [...customs, theme]);
    if (!draft.id) onSetting("theme", theme.id);
    setDraft(null);
    toast(`Tema ${name} salvo`, "success");
  }
  function renameCustom() {
    if (!renaming) return;
    const name = renaming.name.trim();
    if (name) saveCustoms(customs.map((c) => (c.id === renaming.id ? { ...c, name } : c)));
    setRenaming(null);
  }
  function deleteCustom(id: string) {
    saveCustoms(customs.filter((c) => c.id !== id));
    if (pref === id) onSetting("theme", "system");
    if (draft?.id === id) setDraft(null);
    setDeleting(null);
    toast("Tema excluído", "success");
  }

  // Preview: o tema em edição, ou o escolhido; o destaque próprio entra por cima, como no app.
  const shown = draft ? { id: draft.base, vars: customVars(draft) } : themeStyle(pref, customs);
  const baseColors = useMemo(() => readColors(shown.id), [shown.id]);
  const colors = draft?.colors ?? customs.find((c) => c.id === pref)?.colors ?? baseColors;
  const over = accentOver(shown.id, colors, accent);
  const previewVars = over ? { ...shown.vars, "--blue": over.blue, "--blue-bg": over.blueBg, "--on-blue": over.onBlue } : shown.vars;
  const lowContrast = draft ? TEXT_PAIRS.filter(([a, b]) => contrastRatio(draft.colors[a], draft.colors[b]) < 4.5) : [];
  const blueFit = draft ? fitAccent(draft.colors.blue, draft.colors.surface, draft.colors.selected, draft.colors.text, minContrast(draft.base)).adjusted : false;

  const hidden = hiddenTools(settings.toolbarHidden);
  function toggleTool(id: string, on: boolean) {
    const next = new Set(hidden);
    if (on) next.delete(id); else next.add(id);
    onSetting("toolbarHidden", [...next].join(","));
  }

  const cards: { id: string; name: string; custom?: CustomTheme }[] = [
    { id: "system", name: "Seguir o sistema" }, ...THEMES, ...customs.map((c) => ({ id: c.id, name: c.name, custom: c })),
  ];
  return (
    <div className="appearance">
      <section className="settings-group appearance-opts" aria-label="Aparência">
        <h2 className="settings-h">Tema</h2>
        <div className="settings-col">
          <div className="settings-strong" id="theme-label">Temas disponíveis</div>
          <div className="settings-sub">Troca na hora e fica salvo. Alto contraste usa texto AAA (7:1).</div>
          <div className="theme-grid" role="radiogroup" aria-labelledby="theme-label" ref={group} onKeyDown={onKey}>
            {cards.map(({ id, name, custom }) => (
              <button key={id} type="button" role="radio" className="theme-card" aria-checked={id === pref}
                tabIndex={id === pref ? 0 : -1} onClick={() => pick(id, name)}>
                {id === "system" ? (
                  <span className="theme-thumb split-thumb">
                    <span data-theme="light"><Mini /></span>
                    <span data-theme="dark"><Mini /></span>
                  </span>
                ) : (
                  <span className="theme-thumb" data-theme={custom ? custom.base : id} style={custom ? customVars(custom) : undefined}><Mini /></span>
                )}
                <span className="theme-name">{name}</span>
                <span className="theme-check" aria-hidden><Check size={11} weight="bold" /></span>
              </button>
            ))}
          </div>
        </div>
        <div className="settings-col">
          <div className="settings-strong">Temas personalizados</div>
          <div className="settings-sub">Parte do tema atual e muda as cores que você quiser; o resto vem do tema de origem.</div>
          {customs.map((c) => (
            <div key={c.id} className="custom-row">
              {renaming?.id === c.id ? (
                <>
                  <input className="custom-name-input" aria-label={`Novo nome de ${c.name}`} value={renaming.name} maxLength={40} autoFocus
                    onChange={(e) => setRenaming({ id: c.id, name: e.target.value })}
                    onKeyDown={(e) => { if (e.key === "Enter") renameCustom(); }} />
                  <button type="button" className="btn small" onClick={renameCustom}>Salvar</button>
                  <button type="button" className="btn small ghost" onClick={() => setRenaming(null)}>Cancelar</button>
                </>
              ) : deleting === c.id ? (
                <>
                  <span className="custom-name">Excluir "{c.name}"?</span>
                  <button type="button" className="btn small danger" onClick={() => deleteCustom(c.id)}>Excluir</button>
                  <button type="button" className="btn small ghost" onClick={() => setDeleting(null)} autoFocus>Cancelar</button>
                </>
              ) : (
                <>
                  <span className="custom-name">{c.name}</span>
                  <button type="button" className="btn small ghost" aria-label={`Renomear ${c.name}`} onClick={() => setRenaming({ id: c.id, name: c.name })}>Renomear</button>
                  <button type="button" className="btn small ghost" aria-label={`Editar ${c.name}`} onClick={() => setDraft({ id: c.id, name: c.name, base: c.base, colors: { ...c.colors } })}>Editar</button>
                  <button type="button" className="btn small ghost" aria-label={`Excluir ${c.name}`} onClick={() => setDeleting(c.id)}>Excluir</button>
                </>
              )}
            </div>
          ))}
          {!draft && (
            <div>
              <button type="button" className="btn small" disabled={customs.length >= MAX_CUSTOM} onClick={startDraft}>Personalizar</button>
            </div>
          )}
          {draft && (
            <div className="theme-editor" role="group" aria-label="Editor de tema personalizado">
              <label className="theme-editor-name">
                <span>Nome do tema</span>
                <input value={draft.name} maxLength={40} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
              </label>
              <div className="theme-editor-colors">
                {TOKENS.map(([key, css, label]) => (
                  <label key={key} className="color-field">
                    <input type="color" value={draft.colors[key]} data-token={css} onChange={(e) => setDraft({ ...draft, colors: { ...draft.colors, [key]: e.target.value } })} />
                    <span>{label}</span>
                  </label>
                ))}
              </div>
              {lowContrast.map(([a, b]) => (
                <div key={a + b} className="theme-warn" role="status">
                  Contraste baixo: {LABEL[a]} sobre {LABEL[b]} ({contrastRatio(draft.colors[a], draft.colors[b]).toFixed(1).replace(".", ",")}:1; o ideal é 4,5:1 ou mais).
                </div>
              ))}
              {accent
                ? <div className="theme-warn" role="status">A cor de destaque escolhida abaixo vale por cima do Destaque deste tema; use "Do tema" para usar a deste tema.</div>
                : blueFit && <div className="theme-warn" role="status">O destaque será ajustado automaticamente para ter contraste suficiente.</div>}
              <div className="settings-sub">Origem: {THEMES.find((t) => t.id === draft.base)?.name}</div>
              <div className="theme-editor-actions">
                <button type="button" className="btn small primary" onClick={saveDraft}>Salvar tema</button>
                <button type="button" className="btn small ghost" onClick={() => setDraft(null)}>Cancelar</button>
              </div>
            </div>
          )}
        </div>
        <Row title="Cor de destaque" sub={accent && accentAdjusted ? "Ajustada para manter o contraste do tema" : "Links, seleção e data de não lidas. Vazio usa a cor do tema"}>
          <div className="accent-pick">
            {ACCENTS.map(([hex, name]) => (
              <button key={hex} type="button" className="accent-dot" style={{ background: hex }} aria-label={`Destaque ${name}`}
                aria-pressed={accent === hex} onClick={() => setAccent(hex)} />
            ))}
            <input type="color" aria-label="Cor de destaque personalizada" value={accent || "#1f6c9f"} onChange={(e) => dragAccent(e.target.value)} />
            <button type="button" className="btn small" disabled={!accent} onClick={() => setAccent("")}>Do tema</button>
          </div>
        </Row>

        <h2 className="settings-h">Barra lateral</h2>
        <Row title="Densidade das linhas" sub="Altura dos itens e das contas; não muda a lista de mensagens">
          <Segmented label="Densidade das linhas da barra lateral" value={settings.railDensity} fallback="medium" onChange={(v) => onSetting("railDensity", v)}
            options={[["compact", "Compacta"], ["medium", "Média"], ["comfortable", "Confortável"]]} />
        </Row>
        <Row title="Contas" sub="Uma por vez fecha a conta anterior ao abrir outra">
          <Segmented label="Contas abertas na barra lateral" value={settings.railAccounts} fallback="many" onChange={(v) => onSetting("railAccounts", v)}
            options={[["many", "Várias abertas"], ["one", "Uma por vez"]]} />
        </Row>

        <h2 className="settings-h">Lista</h2>
        <Row title="Densidade da lista">
          <Segmented label="Densidade da lista" value={settings.density} fallback="medium" onChange={(v) => onSetting("density", v)}
            options={[["compact", "Compacta"], ["medium", "Média"], ["comfortable", "Confortável"]]} />
        </Row>
        <Row title="Tamanho da fonte" sub="Lista de mensagens e texto simples; emails em HTML mantêm o estilo de origem">
          <Segmented label="Tamanho da fonte" value={settings.size} fallback="m" onChange={(v) => onSetting("size", v)}
            options={[["s", "P"], ["m", "M"], ["l", "G"]]} />
        </Row>
        <Row title="Barra de ferramentas" sub="Altura dos botões e dos ícones">
          <Segmented label="Tamanho da barra de ferramentas" value={settings.toolbar} fallback="m" onChange={(v) => onSetting("toolbar", v)}
            options={[["s", "Compacta"], ["m", "Média"], ["l", "Grande"]]} />
        </Row>
        <Row title="Rótulos da barra" sub="Automático mostra os nomes conforme a largura da janela">
          <Segmented label="Rótulos da barra" value={settings.toolbarLabels} fallback="auto" onChange={(v) => onSetting("toolbarLabels", v)}
            options={[["auto", "Automático"], ["always", "Sempre"], ["icons", "Só ícones"]]} />
        </Row>
        <div className="settings-col">
          <div className="settings-strong">Botões da barra</div>
          <div className="settings-sub">Desmarque o que você não usa; continua disponível no menu de contexto e nos atalhos</div>
          <div className="tool-picks">
            {TOOLBAR_ITEMS.map(([id, label]) => (
              <label key={id} className="tool-pick">
                <input type="checkbox" checked={!hidden.has(id)} onChange={(e) => toggleTool(id, e.target.checked)} /> {label}
              </label>
            ))}
          </div>
        </div>

        <h2 className="settings-h">Estilo</h2>
        <Row title="Cantos" sub="Arredondamento de botões, janelas e menus">
          <Segmented label="Cantos" value={settings.radius} fallback="medium" onChange={(v) => onSetting("radius", v)}
            options={[["sharp", "Retos"], ["soft", "Suaves"], ["medium", "Médios"], ["round", "Arredondados"]]} />
        </Row>
        <Row title="Sombras" sub="Menus, janelas e botão selecionado">
          <Segmented label="Sombras" value={settings.shadow} fallback="default" onChange={(v) => onSetting("shadow", v)}
            options={[["none", "Nenhuma"], ["soft", "Suave"], ["default", "Padrão"], ["strong", "Forte"]]} />
        </Row>
        <Row title="Fonte da interface" sub="Emails em HTML mantêm a fonte de origem; Inter usa a instalada no sistema">
          <Segmented label="Fonte da interface" value={settings.font} fallback="geist" onChange={(v) => onSetting("font", v)}
            options={[["geist", "Geist"], ["system", "Sistema"], ["inter", "Inter"], ["serif", "Serifada"]]} />
        </Row>
      </section>
      <aside className="appearance-preview" aria-label="Pré-visualização">
        <div className="settings-strong">Pré-visualização</div>
        <AppearancePreview theme={shown.id} vars={previewVars} prefs={settings} />
      </aside>
    </div>
  );
}
