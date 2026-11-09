// Mapa de atalhos: transforma o evento de teclado no nome de uma ação. Estilo Outlook (Delete, Ctrl+R, Ctrl+Shift+R, Ctrl+F,
// Ctrl+N, Ctrl+E, Ctrl+Z, Ctrl+Q/U, Insert) mais as teclas simples que já existiam (j/k, r, a, f, c, e, #, s, u, p, /).
export type ShortcutAction =
  | "next" | "prev" | "first" | "last" | "open" | "search" | "reply" | "replyAll" | "forward" | "compose"
  | "archive" | "delete" | "star" | "read" | "unread" | "pin" | "undo";

/** Pontos em que as teclas pertencem a outro controle (menus, diálogos, campos). */
export function ownsKeys(t: EventTarget | null): boolean {
  if (!(t instanceof Element)) return false;
  return !!t.closest("[role=menu], [role=dialog], [role=alertdialog], [role=tablist], [role=radiogroup], .contact-card, .scrim");
}

export const isTyping = (t: EventTarget | null) =>
  t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement || (t instanceof HTMLElement && t.isContentEditable);

/** Ação do atalho, ou null. `typing`: o foco está num campo (só Ctrl+E vale). */
export function shortcutOf(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">, typing: boolean): ShortcutAction | null {
  if (e.altKey) return null;
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (e.ctrlKey || e.metaKey) {
    if (k === "e" && !e.shiftKey) return "search";
    if (typing) return null;
    if (k === "r") return e.shiftKey ? "replyAll" : "reply";
    if (k === "f" && !e.shiftKey) return "forward";
    if (k === "n" && !e.shiftKey) return "compose";
    if (k === "z" && !e.shiftKey) return "undo";
    if (k === "q" && !e.shiftKey) return "read";
    if (k === "u" && !e.shiftKey) return "unread";
    return null;
  }
  if (typing) return null;
  switch (e.key) {
    case "Delete": return "delete";
    case "Insert": return "star";
    case "ArrowDown": return "next";
    case "ArrowUp": return "prev";
    case "Home": return "first";
    case "End": return "last";
    case "Enter": return "open";
    case "#": return "delete";
  }
  if (e.shiftKey && e.key !== "#") return null;
  switch (k) {
    case "j": return "next";
    case "k": return "prev";
    case "/": return "search";
    case "r": return "reply";
    case "a": return "replyAll";
    case "f": return "forward";
    case "c": return "compose";
    case "e": return "archive";
    case "s": return "star";
    case "u": return "unread";
    case "p": return "pin";
  }
  return null;
}
