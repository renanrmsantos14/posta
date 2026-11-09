import { useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "./api";
import { toast } from "./toast";

// Sem scripts, sem formulários, sem fontes externas; imagens só data:/cid: até o usuário liberar a rede.
// O srcdoc herda a CSP do app (tauri.conf.json), que por isso também precisa permitir https:/http: em img-src.
const csp = (remote: boolean) =>
  `default-src 'none'; img-src data: cid:${remote ? " https: http:" : ""}; style-src 'unsafe-inline'; form-action 'none'; base-uri 'none'`;

const REMOTE = /\b(?:src|srcset|background|poster)\s*=\s*["']?\s*https?:|url\(\s*["']?https?:|@import/i;
const STRIP = "script, meta, base, link, iframe, frame, object, embed, form, noscript";
const MAX_HEIGHT = 30000;

/** Tema claro fixo: email HTML traz as próprias cores e quase nunca funciona no escuro. */
const BASE_CSS = `html{color-scheme:light}body{margin:0;padding:4px 0;background:#fff;color:#1b1b1b;
font:15px/1.55 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;overflow-wrap:anywhere;word-break:break-word}
img{max-width:100%;height:auto}table{max-width:100%}a{color:#1f6c9f}pre{white-space:pre-wrap}`;

export function hasRemoteImages(html: string) {
  return REMOTE.test(html);
}

function buildDoc(html: string, remote: boolean) {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.querySelectorAll(STRIP).forEach((n) => n.remove());
  doc.querySelectorAll("a[target]").forEach((a) => a.removeAttribute("target"));
  const head = `<meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp(remote)}"><style>${BASE_CSS}</style>`;
  return `<!doctype html><html><head>${head}${doc.head.innerHTML}</head>${doc.body.outerHTML}</html>`;
}

/** Documentos já montados (sanitização + CSP): reabrir uma mensagem recente não refaz o parse. */
const DOC_CACHE_MAX = 40;
const docCache = new Map<string, string>();
function cachedDoc(html: string, remote: boolean) {
  const key = (remote ? "1" : "0") + html;
  const hit = docCache.get(key);
  if (hit !== undefined) {
    docCache.delete(key);
    docCache.set(key, hit);
    return hit;
  }
  const doc = buildDoc(html, remote);
  docCache.set(key, doc);
  if (docCache.size > DOC_CACHE_MAX) docCache.delete(docCache.keys().next().value as string);
  return doc;
}

export async function openExternal(url: string) {
  if (isTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

/**
 * Email HTML num iframe isolado. `sandbox="allow-same-origin"` (sem allow-scripts) mantém scripts
 * desligados e deixa só o app ler o documento para medir a altura e capturar cliques nos links.
 */
export function HtmlBody({ html, remote, onContext }: { html: string; remote: boolean; onContext?: (x: number, y: number, selection: string) => void }) {
  const ref = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(120);
  const srcDoc = useMemo(() => cachedDoc(html, remote), [html, remote]);
  const ctx = useRef(onContext);
  ctx.current = onContext;

  useEffect(() => {
    const frame = ref.current;
    if (!frame) return;
    let observer: ResizeObserver | undefined;
    const wired = new WeakSet<Document>();
    const attach = () => {
      const doc = frame.contentDocument;
      if (!doc?.documentElement || !doc.body) return;
      observer?.disconnect();
      const fit = () => setHeight(Math.min(MAX_HEIGHT, Math.ceil(doc.documentElement.scrollHeight)));
      observer = new ResizeObserver(fit);
      observer.observe(doc.documentElement);
      observer.observe(doc.body);
      fit();
      if (wired.has(doc)) return;
      wired.add(doc);
      doc.addEventListener("contextmenu", (e) => {
        if (!ctx.current) return;
        e.preventDefault();
        const r = frame.getBoundingClientRect();
        ctx.current(r.left + e.clientX, r.top + e.clientY, doc.getSelection()?.toString() ?? "");
      });
      doc.addEventListener("click", (e) => {
        const a = (e.target as Element | null)?.closest?.("a[href], area[href]") as HTMLAnchorElement | null;
        if (!a) return;
        e.preventDefault();
        // `.href` já vem sem espaços/quebras de linha que newsletters deixam no atributo.
        const href = a.href.trim();
        if (!/^(https?:|mailto:|tel:)/i.test(href)) return;
        openExternal(href).catch((err) => toast(`Não foi possível abrir o link: ${String(err)}`, "error"));
      });
    };
    frame.addEventListener("load", attach);
    attach();
    return () => {
      frame.removeEventListener("load", attach);
      observer?.disconnect();
    };
  }, [srcDoc]);

  return (
    <iframe ref={ref} className="html-body" title="Conteúdo da mensagem" sandbox="allow-same-origin"
      srcDoc={srcDoc} style={{ height }} />
  );
}
