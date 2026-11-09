// Configurações em página inteira: menu de seções à esquerda; voltar (botão ou Esc) devolve a caixa de entrada.
import { useEffect, useState } from "react";
import { AirplaneTilt, ArrowLeft, ArrowsClockwise, Bell, ChatTeardropText, FunnelSimple, Gauge, Keyboard, PencilSimple, Plus, Tag, Trash, UserCircle, Layout, Desktop, WifiSlash, X } from "@phosphor-icons/react";
import { api, providerOf, PROVIDER_LABEL, type Account, type Settings } from "./api";
import { OauthAdvanced } from "./OauthSettings";
import { AccountEditDialog } from "./AccountEditDialog";
import { RulesSection, TemplatesSection, VacationSection } from "./ProductivitySettings";
import { MAX_CATEGORIES, PALETTE, parseCategories } from "./Categories";
import { autoCheck, checkUpdate, setAutoCheck, type Update } from "./updater";
import { aliasLines } from "./aliases";
import { reauthenticate, toast } from "./toast";
import build from "./version.json";

interface Props {
  accounts: Account[];
  settings: Settings;
  onSetting: (key: string, value: string) => void;
  /** Resolve quando a conta foi removida; rejeita com a mensagem de erro. */
  onRemoveAccount: (id: string) => Promise<void>;
  onAddAccount: () => void;
  /** Conta editada e já gravada (a lista do app troca a antiga por esta). */
  onAccountSaved: (a: Account) => void;
  onClose: () => void;
  onUpdate: (u: Update) => void;
  /** "Aplicar agora à Entrada": roda as regras e atualiza a lista. */
  onApplyRules: () => Promise<void>;
}

const SECTIONS = [
  { id: "accounts", label: "Contas", icon: UserCircle },
  { id: "categories", label: "Categorias", icon: Tag },
  { id: "rules", label: "Regras", icon: FunnelSimple },
  { id: "templates", label: "Respostas prontas", icon: ChatTeardropText },
  { id: "vacation", label: "Férias", icon: AirplaneTilt },
  { id: "layout", label: "Leitura e layout", icon: Layout },
  { id: "write", label: "Escrita", icon: PencilSimple },
  { id: "notify", label: "Notificações", icon: Bell },
  { id: "system", label: "Sistema", icon: Desktop },
  { id: "offline", label: "Offline", icon: WifiSlash },
  { id: "keys", label: "Atalhos", icon: Keyboard },
  { id: "about", label: "Atualizações", icon: Gauge },
] as const;
type SectionId = (typeof SECTIONS)[number]["id"];

const SHORTCUTS: [string, string][] = [
  ["j / ↓", "Próxima mensagem"], ["k / ↑", "Mensagem anterior"], ["/ / Ctrl+E", "Buscar"],
  ["r / Ctrl+R", "Responder"], ["a / Ctrl+Shift+R", "Responder a todos"], ["f / Ctrl+F", "Encaminhar"], ["c / Ctrl+N", "Escrever"],
  ["e", "Arquivar"], ["# / Delete", "Excluir"], ["s / Insert", "Sinalizar com estrela"], ["u / Ctrl+U", "Marcar como não lida"],
  ["Ctrl+Q", "Marcar como lida"], ["Ctrl+Z", "Desfazer a última ação"], ["Botão direito", "Menu de contexto na lista, nas pastas e na leitura"],
  ["Shift + clique", "Marcar várias mensagens"], ["Clique duplo", "Abrir a mensagem em tela cheia"],
  ["Esc", "Sair da tela cheia, limpar a busca ou a seleção"],
  ["p", "Fixar no topo"],
  ["de: para: assunto: tem:anexo é:não-lida antes: depois:", "Filtros da busca"],
];

const UNDO_OPTIONS = ["4", "10", "20", "30"];
const SWIPE_OPTIONS: [string, string][] = [["archive", "Arquivar"], ["trash", "Excluir"], ["read", "Lida/não lida"], ["none", "Nada"]];
/** Bandeja e iniciar com o sistema só existem no desktop. */
const DESKTOP = !/Android/i.test(navigator.userAgent);
const OFFLINE_OPTIONS = ["50", "200", "500", "1000"];

/** Grupo de opções exclusivas; valor ausente ou desconhecido cai no `fallback`. */
function Segmented({ label, value, fallback, options, onChange }: {
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

function Row({ title, sub, children }: { title: string; sub?: string; children: React.ReactNode }) {
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

export function SettingsPage({ accounts, settings, onSetting, onRemoveAccount, onAddAccount, onAccountSaved, onClose, onUpdate, onApplyRules }: Props) {
  const [section, setSection] = useState<SectionId>("accounts");
  const [auto, setAuto] = useState(autoCheck);
  const [checking, setChecking] = useState(false);
  const [sigAccount, setSigAccount] = useState(accounts[0]?.id ?? "");
  const [editing, setEditing] = useState<Account | null>(null);
  const [removing, setRemoving] = useState<Account | null>(null);
  const [removeBusy, setRemoveBusy] = useState(false);
  const notify = settings.notify !== "0";
  const [autostart, setAutostartOn] = useState<boolean | null>(null);
  useEffect(() => {
    if (DESKTOP) api.getAutostart().then(setAutostartOn).catch(() => setAutostartOn(false));
  }, []);
  async function toggleAutostart(on: boolean) {
    try {
      await api.setAutostart(on);
      setAutostartOn(on);
    } catch (e) {
      toast(`Não consegui mudar o início com o sistema: ${e}`, "error");
    }
  }
  const sigId = accounts.some((a) => a.id === sigAccount) ? sigAccount : accounts[0]?.id ?? "";

  const categories = parseCategories(settings.categories);
  const saveCategories = (list: typeof categories) => onSetting("categories", JSON.stringify(list));
  function addCategory() {
    const color = PALETTE.find((p) => !categories.some((c) => c.color === p.key))?.key ?? PALETTE[0].key;
    saveCategories([...categories, { id: `c${Date.now().toString(36)}`, name: "Nova categoria", color }]);
  }

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

  async function check() {
    setChecking(true);
    try {
      const found = await checkUpdate();
      if (found) { onClose(); onUpdate(found); }
      else toast("Você está na versão mais recente", "success");
    } catch (e) {
      toast(`Não consegui procurar atualizações: ${e}`, "error");
    } finally {
      setChecking(false);
    }
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      if (editing) return; // o diálogo de edição trata o próprio Esc
      if (removing) { if (!removeBusy) setRemoving(null); }
      else onClose();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  const title = SECTIONS.find((s) => s.id === section)!.label;

  return (
    <div className="settings-page">
      <nav className="settings-nav" aria-label="Seções das configurações">
        <button type="button" className="btn ghost small settings-back" onClick={onClose}>
          <ArrowLeft size={14} weight="bold" /> Voltar <kbd>Esc</kbd>
        </button>
        {SECTIONS.map(({ id, label, icon: Icon }) => (
          <button key={id} type="button" className="nav-item" aria-current={id === section ? "page" : undefined} onClick={() => setSection(id)}>
            <Icon size={16} weight="bold" /> <span>{label}</span>
          </button>
        ))}
      </nav>

      <main className="settings-main">
        <div className="settings-body" key={section}>
          <h1>{title}</h1>

          {section === "accounts" && (
            <section className="settings-group" aria-label="Contas">
              {accounts.length === 0 && <p className="settings-sub">Nenhuma conta conectada.</p>}
              {accounts.map((a) => (
                <div key={a.id} className="settings-row">
                  <div className="settings-account">
                    <div className="settings-strong">{a.name ? `${a.name} · ${a.username}` : a.username}</div>
                    <div className="settings-sub">{providerOf(a) ? `Conta ${PROVIDER_LABEL[providerOf(a)!]} · ` : ""}{a.host}</div>
                    <label className="settings-sub alias-field">Remetentes alternativos (um por linha: <code>Nome &lt;endereço&gt;</code> ou só o endereço)
                      <textarea className="signature-input" rows={Math.max(2, aliasLines(settings[`aliases:${a.id}`]).length + 1)} aria-label={`Remetentes alternativos de ${a.username}`}
                        placeholder="reservas@exemplo.com" value={settings[`aliases:${a.id}`] ?? ""}
                        onChange={(e) => onSetting(`aliases:${a.id}`, e.target.value)} />
                    </label>
                  </div>
                  {providerOf(a) && (
                    <button type="button" className="btn" onClick={() => reauthenticate(a.id)}>Entrar de novo</button>
                  )}
                  <button type="button" className="btn" onClick={() => setEditing(a)}>
                    <PencilSimple size={15} weight="bold" /> Editar
                  </button>
                  <button type="button" className="btn" onClick={() => setRemoving(a)}>
                    <Trash size={15} weight="bold" /> Remover
                  </button>
                </div>
              ))}
              <div className="settings-row">
                <button type="button" className="btn" onClick={onAddAccount}><Plus size={15} weight="bold" /> Adicionar conta</button>
              </div>
              <OauthAdvanced />
            </section>
          )}

          {section === "categories" && (
            <section className="settings-group" aria-label="Categorias">
              <p className="settings-sub">Até {MAX_CATEGORIES} categorias coloridas. Aplique pelo botão de categoria na mensagem; elas ficam só neste aparelho.</p>
              {categories.map((c) => (
                <div key={c.id} className="settings-row cat-row">
                  <div className="cat-swatches" role="radiogroup" aria-label={`Cor de ${c.name}`}>
                    {PALETTE.map(({ key, label }) => (
                      <button key={key} type="button" role="radio" aria-checked={c.color === key} aria-label={label} title={label}
                        className="cat-swatch" style={{ background: `var(--${key}-bg)`, color: `var(--${key})` }} onClick={() => saveCategories(categories.map((x) => (x.id === c.id ? { ...x, color: key } : x)))} />
                    ))}
                  </div>
                  <input className="cat-name" aria-label="Nome da categoria" maxLength={24} value={c.name}
                    onChange={(e) => saveCategories(categories.map((x) => (x.id === c.id ? { ...x, name: e.target.value } : x)))} />
                  <button type="button" className="icon-btn" aria-label={`Remover ${c.name}`} onClick={() => saveCategories(categories.filter((x) => x.id !== c.id))}>
                    <X size={14} weight="bold" />
                  </button>
                </div>
              ))}
              <div className="settings-row">
                <button type="button" className="btn" disabled={categories.length >= MAX_CATEGORIES} onClick={addCategory}>
                  <Plus size={15} weight="bold" /> Adicionar categoria
                </button>
              </div>
            </section>
          )}

          {section === "rules" && <RulesSection settings={settings} onSetting={onSetting} onApply={onApplyRules} />}
          {section === "templates" && <TemplatesSection settings={settings} onSetting={onSetting} />}
          {section === "vacation" && <VacationSection settings={settings} onSetting={onSetting} />}

          {section === "layout" && (
            <section className="settings-group" aria-label="Leitura e layout">
              <Row title="Tema" sub="Sistema segue a aparência do aparelho">
                <Segmented label="Tema" value={settings.theme} fallback="system" onChange={(v) => onSetting("theme", v)}
                  options={[["system", "Sistema"], ["light", "Claro"], ["dark", "Escuro"]]} />
              </Row>
              <Row title="Tamanho da fonte" sub="Lista de mensagens e texto simples; emails em HTML mantêm o estilo de origem">
                <Segmented label="Tamanho da fonte" value={settings.size} fallback="m" onChange={(v) => onSetting("size", v)}
                  options={[["s", "P"], ["m", "M"], ["l", "G"]]} />
              </Row>
              <Row title="Densidade da lista">
                <Segmented label="Densidade da lista" value={settings.density} fallback="medium" onChange={(v) => onSetting("density", v)}
                  options={[["compact", "Compacta"], ["medium", "Média"], ["comfortable", "Confortável"]]} />
              </Row>
              <Row title="Foco e Outros" sub="Separa a Caixa de entrada em Foco (pessoas e conversas suas) e Outros (newsletters e avisos automáticos)">
                <Segmented label="Foco e Outros" value={settings.focusTabs} fallback="1" onChange={(v) => onSetting("focusTabs", v)}
                  options={[["1", "Ligado"], ["0", "Desligado"]]} />
              </Row>
              <Row title="Prévia do texto na lista" sub="Primeira linha da mensagem sob o assunto">
                <Segmented label="Prévia do texto" value={settings.preview} fallback="1" onChange={(v) => onSetting("preview", v)}
                  options={[["1", "Mostrar"], ["0", "Ocultar"]]} />
              </Row>
              <Row title="Agrupar por conversa" sub="Respostas do mesmo assunto viram uma linha; o painel mostra o fio inteiro">
                <Segmented label="Agrupar por conversa" value={settings.group} fallback="1" onChange={(v) => onSetting("group", v)}
                  options={[["1", "Sim"], ["0", "Não"]]} />
              </Row>
              <Row title="Painel de leitura" sub="Desligado abre a mensagem em tela cheia ao clicar">
                <Segmented label="Painel de leitura" value={settings.pane} fallback="right" onChange={(v) => onSetting("pane", v)}
                  options={[["right", "Direita"], ["bottom", "Embaixo"], ["off", "Desligado"]]} />
              </Row>
              <Row title="Deslizar para a direita" sub="No toque: arrasta a linha da lista. Excluir e arquivar têm Desfazer">
                <Segmented label="Deslizar para a direita" value={settings.swipeRight} fallback="archive" onChange={(v) => onSetting("swipeRight", v)} options={SWIPE_OPTIONS} />
              </Row>
              <Row title="Deslizar para a esquerda">
                <Segmented label="Deslizar para a esquerda" value={settings.swipeLeft} fallback="trash" onChange={(v) => onSetting("swipeLeft", v)} options={SWIPE_OPTIONS} />
              </Row>
              <Row title="Larguras das colunas" sub="Arraste a borda entre os painéis; clique duplo na borda restaura o padrão">
                <button type="button" className="btn" onClick={() => ["w:rail", "w:list", "h:list"].forEach((k) => onSetting(k, ""))}>Restaurar padrão</button>
              </Row>
            </section>
          )}

          {section === "write" && (
            <section className="settings-group" aria-label="Escrita">
              <Row title="Desfazer envio" sub="Tempo para desfazer antes da mensagem sair de fato">
                <Segmented label="Desfazer envio" value={settings.undoSend} fallback="4" onChange={(v) => onSetting("undoSend", v)}
                  options={UNDO_OPTIONS.map((s): [string, string] => [s, `${s} s`])} />
              </Row>
              {accounts.length > 0 && (
                <div className="settings-col">
                  <div className="settings-strong">Assinatura</div>
                  {accounts.length > 1 && (
                    <select aria-label="Conta da assinatura" value={sigId} onChange={(e) => setSigAccount(e.target.value)}>
                      {accounts.map((a) => <option key={a.id} value={a.id}>{a.username}</option>)}
                    </select>
                  )}
                  <textarea className="signature-input" rows={4} aria-label="Assinatura" placeholder="Entra no fim das mensagens novas e das respostas"
                    value={settings[`signature:${sigId}`] ?? ""} onChange={(e) => onSetting(`signature:${sigId}`, e.target.value)} />
                </div>
              )}
            </section>
          )}

          {section === "notify" && (
            <section className="settings-group" aria-label="Notificações">
              <label className="settings-row switch-row">
                <div>
                  <div className="settings-strong">Avisar de email novo</div>
                  <div className="settings-sub">Notificação do sistema com remetente e assunto, quando a janela não está em foco</div>
                </div>
                <input type="checkbox" role="switch" className="switch" checked={notify} onChange={(e) => toggleNotify(e.target.checked)} />
              </label>
            </section>
          )}

          {section === "system" && (
            <section className="settings-group" aria-label="Sistema">
              {DESKTOP ? (
                <>
                  <label className="settings-row switch-row">
                    <div>
                      <div className="settings-strong">Fechar a janela mantém o Posta na bandeja</div>
                      <div className="settings-sub">Continua recebendo e avisando de email novo. Sair de vez: menu da bandeja</div>
                    </div>
                    <input type="checkbox" role="switch" className="switch" checked={settings.closeToTray !== "0"}
                      onChange={(e) => onSetting("closeToTray", e.target.checked ? "1" : "0")} />
                  </label>
                  <label className="settings-row switch-row">
                    <div>
                      <div className="settings-strong">Iniciar com o sistema</div>
                      <div className="settings-sub">Abre escondido na bandeja ao entrar no computador</div>
                    </div>
                    <input type="checkbox" role="switch" className="switch" checked={!!autostart} disabled={autostart === null}
                      onChange={(e) => toggleAutostart(e.target.checked)} />
                  </label>
                </>
              ) : (
                <p className="settings-sub">Bandeja e início com o sistema valem só para o computador.</p>
              )}
            </section>
          )}

          {section === "offline" && (
            <section className="settings-group" aria-label="Offline">
              <Row title="Leitura offline" sub="Quantas mensagens recentes da Entrada ficam baixadas, com o corpo, para ler sem conexão. Anexos só abrem com rede.">
                <Segmented label="Mensagens para leitura offline" value={settings.offlineN} fallback="200" onChange={(v) => onSetting("offlineN", v)}
                  options={OFFLINE_OPTIONS.map((n): [string, string] => [n, n])} />
              </Row>
              <div className="settings-sub">Sem rede, ler, marcar, mover e excluir continuam funcionando: as ações esperam numa fila e seguem para o servidor quando a conexão volta. O que você escrever vai para a Caixa de saída.</div>
            </section>
          )}

          {section === "keys" && (
            <section className="settings-group" aria-label="Atalhos">
              {SHORTCUTS.map(([keys, what]) => (
                <div key={keys} className="settings-row">
                  <div>{what}</div>
                  <span className="keys">{keys.split(" / ").map((k) => <kbd key={k}>{k}</kbd>)}</span>
                </div>
              ))}
            </section>
          )}

          {section === "about" && (
            <section className="settings-group" aria-label="Atualizações">
              <Row title={`Posta v${build.version}`} sub={`Compilado em ${build.builtAt}`}>
                <button type="button" className="btn" onClick={check} disabled={checking} data-busy={checking || undefined}>
                  <ArrowsClockwise size={15} weight="bold" className="sync-icon" />
                  {checking ? "Procurando…" : "Procurar atualizações"}
                </button>
              </Row>
              <label className="settings-row switch-row">
                <div>
                  <div className="settings-strong">Procurar automaticamente</div>
                  <div className="settings-sub">Ao abrir o app e a cada 6 horas</div>
                </div>
                <input type="checkbox" role="switch" className="switch" checked={auto}
                  onChange={(e) => { setAuto(e.target.checked); setAutoCheck(e.target.checked); }} />
              </label>
            </section>
          )}
        </div>
      </main>

      {editing && <AccountEditDialog account={editing} onClose={() => setEditing(null)} onSaved={onAccountSaved} />}

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
