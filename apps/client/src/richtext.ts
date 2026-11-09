// Texto formatado do Composer: limpeza do HTML colado ou citado e a versão em texto simples para o envio.

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
export { esc as escapeHtml };

/** Texto simples em HTML: uma linha por bloco (a linha vazia vira `<br>`, como o editor faz ao dar Enter). */
export function textToHtml(text: string): string {
  return text.split("\n").map((l) => `<div>${l === "" ? "<br>" : esc(l)}</div>`).join("");
}

// Só estrutura e ênfase. O resto (estilos, classes, ids, imagens, eventos) é descartado.
const KEEP = new Set(["B", "STRONG", "I", "EM", "U", "S", "STRIKE", "DEL", "BR", "P", "DIV", "UL", "OL", "LI", "A", "BLOCKQUOTE",
  "H1", "H2", "H3", "H4", "H5", "H6", "PRE", "CODE", "HR", "TABLE", "THEAD", "TBODY", "TR", "TD", "TH"]);
// Some com o conteúdo junto; qualquer outra tag desconhecida só perde a marcação.
const DROP = new Set(["SCRIPT", "STYLE", "IFRAME", "FRAME", "OBJECT", "EMBED", "FORM", "NOSCRIPT", "TEMPLATE", "HEAD", "TITLE",
  "META", "LINK", "BASE", "SVG", "MATH", "IMG", "VIDEO", "AUDIO", "CANVAS", "INPUT", "BUTTON", "TEXTAREA", "SELECT"]);
const SAFE_HREF = /^(https?:|mailto:|tel:)/i;

function clean(src: Node, into: Node, doc: Document) {
  for (const n of Array.from(src.childNodes)) {
    if (n.nodeType === Node.TEXT_NODE) {
      into.appendChild(doc.createTextNode(n.textContent ?? ""));
    } else if (n.nodeType === Node.ELEMENT_NODE) {
      const el = n as Element;
      if (DROP.has(el.tagName.toUpperCase())) continue;
      if (!KEEP.has(el.tagName.toUpperCase())) {
        clean(el, into, doc);
        continue;
      }
      const copy = doc.createElement(el.tagName.toLowerCase());
      if (el.tagName === "A") {
        const href = el.getAttribute("href")?.trim() ?? "";
        if (SAFE_HREF.test(href)) {
          copy.setAttribute("href", href);
          copy.setAttribute("rel", "noopener noreferrer");
        }
      }
      clean(el, copy, doc);
      into.appendChild(copy);
    }
  }
}

/** HTML seguro para o editor e para o envio: sem script, estilo, imagens externas nem manipuladores de evento. */
export function sanitizeHtml(html: string): string {
  const src = new DOMParser().parseFromString(html, "text/html");
  const out = document.implementation.createHTMLDocument("");
  clean(src.body, out.body, out);
  return out.body.innerHTML;
}

const BLOCK = new Set(["DIV", "P", "UL", "OL", "LI", "H1", "H2", "H3", "H4", "H5", "H6", "PRE", "TABLE", "TR", "HR"]);

function walk(root: Node): string {
  let out = "";
  const newline = () => { if (out && !out.endsWith("\n")) out += "\n"; };
  const visit = (n: Node, listIndex?: { i: number; ordered: boolean }) => {
    if (n.nodeType === Node.TEXT_NODE) {
      const t = (n.textContent ?? "").replace(/ /g, " ").replace(/[ \t\r\n]+/g, " ");
      if (t !== " " || (out && !out.endsWith("\n"))) out += t;
      return;
    }
    if (n.nodeType !== Node.ELEMENT_NODE) return;
    const el = n as Element;
    const tag = el.tagName;
    if (tag === "BR") { out += "\n"; return; }
    if (tag === "HR") { newline(); out += "---\n"; return; }
    if (tag === "BLOCKQUOTE") {
      const inner = walk(el).replace(/\n+$/, "");
      newline();
      out += inner.split("\n").map((l) => (l ? `> ${l}` : ">")).join("\n") + "\n";
      return;
    }
    if (tag === "UL" || tag === "OL") {
      newline();
      const state = { i: 0, ordered: tag === "OL" };
      el.childNodes.forEach((c) => visit(c, state));
      newline();
      return;
    }
    if (tag === "LI") {
      newline();
      out += listIndex?.ordered ? `${++listIndex.i}. ` : "- ";
    } else if (BLOCK.has(tag)) {
      newline();
    }
    if (tag === "TD" || tag === "TH") {
      el.childNodes.forEach((c) => visit(c));
      out += " ";
      return;
    }
    el.childNodes.forEach((c) => visit(c));
    if (tag === "A") {
      const href = el.getAttribute("href") ?? "";
      const label = (el.textContent ?? "").trim();
      if (href && label && href !== label && href.replace(/^mailto:/i, "") !== label) out += ` (${href})`;
    }
    if (BLOCK.has(tag)) newline();
    if (tag === "P" || /^H\d$/.test(tag)) out += "\n";
  };
  root.childNodes.forEach((c) => visit(c));
  return out;
}

/** Versão em texto simples (parte `text/plain`): listas com "- ", citações com "> ", links com a URL. */
export function htmlToText(html: string): string {
  const body = new DOMParser().parseFromString(html, "text/html").body;
  return walk(body)
    .split("\n")
    .map((l) => (l === "-- " ? l : l.replace(/[ \t]+$/, "")))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/^\n+|\n+$/g, "");
}

/** HTML de saída: citações ganham o recuo e a barra que os leitores de email esperam. */
export function outgoingHtml(html: string): string {
  const safe = sanitizeHtml(html);
  if (!safe.trim()) return "";
  return safe.replace(/<blockquote>/g, '<blockquote style="margin:0 0 0 .8ex;padding-left:1ex;border-left:2px solid #ccc">');
}
