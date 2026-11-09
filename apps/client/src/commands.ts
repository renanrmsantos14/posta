// Comandos da faixa (CommandBar): o que cada botão faz sobre a seleção atual. Puro: tudo que depende do estado entra por `c`.
import { Archive, ArrowBendDoubleUpLeft, ArrowBendUpLeft, ArrowBendUpRight, Clock, EnvelopeSimple, EnvelopeSimpleOpen, FloppyDisk, FolderOpen, NotePencil, Printer, PushPin, Star, Tag, Trash } from "@phosphor-icons/react";
import type { Cmd } from "./CommandBar";
import type { MenuItem } from "./ContextMenu";
import type { MessageDetail, MessageSummary, Role } from "./api";

/** O WebView do Android não tem impressão por `window.print`. */
const ANDROID = /Android/i.test(navigator.userAgent);

export interface CommandContext {
  targets: MessageSummary[];
  current: MessageSummary | MessageDetail | null | undefined;
  hasCategories: boolean;
  canArchive: (m: MessageSummary) => boolean;
  canTrash: (m: MessageSummary) => boolean;
  archiveTo: (m: MessageSummary) => Role;
  writeNew: () => void;
  act: (kind: "reply" | "replyAll" | "forward") => Promise<unknown> | void;
  remove: (ms: MessageSummary[]) => void;
  archive: (ms: MessageSummary[]) => void;
  markSeen: (ms: MessageSummary[], seen: boolean) => unknown;
  toggleStar: (ms: MessageSummary[], on?: boolean) => unknown;
  togglePin: (ms: MessageSummary[]) => unknown;
  moveItems: (ms: MessageSummary[]) => MenuItem[];
  snoozeItems: (ms: MessageSummary[]) => MenuItem[];
  categoryItems: (ms: MessageSummary[]) => MenuItem[];
  saveEml: (id: number) => unknown;
}

export function buildCommands(c: CommandContext): Cmd[] {
  const { targets, current, canArchive, canTrash, archiveTo, writeNew, act, remove, archive, markSeen, toggleStar, togglePin, moveItems, snoozeItems, categoryItems, saveEml } = c;
  const none = targets.length === 0;
  return [
    { id: "new", label: "Novo email", icon: NotePencil, pri: 1, group: 0, kbd: "Ctrl+N", primary: true, run: () => writeNew() },
    { id: "delete", label: targets.length && targets.every((m) => m.role === "trash") ? "Excluir de vez" : "Excluir", icon: Trash, pri: 2, group: 1, kbd: "Delete", disabled: !targets.some(canTrash), run: () => remove(targets) },
    { id: "archive", label: targets.length && targets.some(canArchive) && archiveTo(targets.find(canArchive)!) === "inbox" ? "Mover para a entrada" : "Arquivar", icon: Archive, pri: 3, group: 1, kbd: "E", disabled: !targets.some(canArchive), run: () => archive(targets) },
    { id: "reply", label: "Responder", icon: ArrowBendUpLeft, pri: 4, group: 2, kbd: "Ctrl+R", disabled: !current, run: () => void act("reply") },
    { id: "replyAll", label: "Responder a todos", icon: ArrowBendDoubleUpLeft, pri: 5, group: 2, kbd: "Ctrl+Shift+R", disabled: !current, run: () => void act("replyAll") },
    { id: "forward", label: "Encaminhar", icon: ArrowBendUpRight, pri: 6, group: 2, kbd: "Ctrl+F", disabled: !current, run: () => void act("forward") },
    { id: "move", label: "Mover", icon: FolderOpen, pri: 7, group: 3, disabled: none, menu: () => moveItems(targets) },
    targets.length && targets.every((m) => m.seen)
      ? { id: "read", label: "Marcar não lida", icon: EnvelopeSimple, pri: 8, group: 4, kbd: "Ctrl+U", run: () => void markSeen(targets, false) }
      : { id: "read", label: "Marcar lida", icon: EnvelopeSimpleOpen, pri: 8, group: 4, kbd: "Ctrl+Q", disabled: none, run: () => void markSeen(targets, true) },
    { id: "flag", label: "Sinalizar", icon: Star, pri: 9, group: 4, kbd: "Insert", disabled: none, pressed: !none && targets.every((m) => m.flagged), run: () => void toggleStar(targets, true) },
    { id: "pin", label: "Fixar", icon: PushPin, pri: 10, group: 4, kbd: "P", disabled: none, pressed: !none && targets.every((m) => m.pinned), run: () => void togglePin(targets) },
    { id: "snooze", label: "Adiar", icon: Clock, pri: 11, group: 5, disabled: none, menu: () => snoozeItems(targets) },
    ...(c.hasCategories ? [{ id: "cats", label: "Categorias", icon: Tag, pri: 12, group: 5, disabled: none, menu: () => categoryItems(targets) }] : []),
    ...(!ANDROID ? [{ id: "print", label: "Imprimir", icon: Printer, pri: 13, group: 6, disabled: !current, run: () => window.print() }] : []),
    { id: "eml", label: "Salvar .eml", icon: FloppyDisk, pri: 14, group: 6, disabled: !current, run: () => current && void saveEml(current.id) },
  ];
}
