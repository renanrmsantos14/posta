// Configurações > Aparência: tema (miniaturas clicáveis), cor de destaque, densidade e tamanho da fonte.
import { useEffect, useRef } from "react";
import { Check } from "@phosphor-icons/react";
import type { Settings } from "./api";
import { Row, Segmented } from "./SettingsParts";
import { ACCENTS, THEMES, themePref } from "./themes";
import { TOOLBAR_ITEMS, hiddenTools } from "./commands";
import { toast } from "./toast";

interface Props {
  settings: Settings;
  onSetting: (key: string, value: string) => void;
  onPreview: (key: string, value: string) => void;
  accentAdjusted: boolean;
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

export function AppearanceSection({ settings, onSetting, onPreview, accentAdjusted }: Props) {
  const pref = themePref(settings.theme);
  const group = useRef<HTMLDivElement>(null);
  const accent = /^#[0-9a-f]{6}$/i.test(settings.accent ?? "") ? settings.accent!.toLowerCase() : "";

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

  const hidden = hiddenTools(settings.toolbarHidden);
  function toggleTool(id: string, on: boolean) {
    const next = new Set(hidden);
    if (on) next.delete(id); else next.add(id);
    onSetting("toolbarHidden", [...next].join(","));
  }

  const cards = [{ id: "system", name: "Seguir o sistema", dark: false }, ...THEMES];
  return (
    <section className="settings-group" aria-label="Aparência">
      <div className="settings-col">
        <div className="settings-strong" id="theme-label">Tema</div>
        <div className="settings-sub">Troca na hora e fica salvo. Alto contraste usa texto AAA (7:1).</div>
        <div className="theme-grid" role="radiogroup" aria-labelledby="theme-label" ref={group} onKeyDown={onKey}>
          {cards.map(({ id, name }) => (
            <button key={id} type="button" role="radio" className="theme-card" aria-checked={id === pref}
              tabIndex={id === pref ? 0 : -1} onClick={() => pick(id, name)}>
              {id === "system" ? (
                <span className="theme-thumb split-thumb">
                  <span data-theme="light"><Mini /></span>
                  <span data-theme="dark"><Mini /></span>
                </span>
              ) : (
                <span className="theme-thumb" data-theme={id}><Mini /></span>
              )}
              <span className="theme-name">{name}</span>
              <span className="theme-check" aria-hidden><Check size={11} weight="bold" /></span>
            </button>
          ))}
        </div>
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
    </section>
  );
}
