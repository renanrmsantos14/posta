import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { MagnifyingGlassMinus, MagnifyingGlassPlus } from "@phosphor-icons/react";
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
img{max-width:100%;height:auto}a{color:#1f6c9f}pre{white-space:pre-wrap}`;

/** Zoom do leitor (como no Outlook): multiplica o "ajustar à largura"; fica salvo e vale para todas as mensagens. */
const ZOOM_KEY = "posta.readerZoom";
const ZOOM_STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];
const zoomSubs = new Set<() => void>();
let zoom = (() => {
  try { const v = Number(localStorage.getItem(ZOOM_KEY)); return ZOOM_STEPS.includes(v) ? v : 1; } catch { return 1; }
})();
function setReaderZoom(next: number) {
  zoom = next;
  try { localStorage.setItem(ZOOM_KEY, String(next)); } catch { /* sem armazenamento: vale só nesta sessão */ }
  zoomSubs.forEach((f) => f());
}
/** dir 0 volta a 100%; 1/-1 anda um degrau. */
function stepReaderZoom(dir: 1 | -1 | 0) {
  if (dir === 0) return setReaderZoom(1);
  const i = ZOOM_STEPS.indexOf(zoom);
  setReaderZoom(ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, i + dir))]);
}
const useReaderZoom = () => useSyncExternalStore((f) => (zoomSubs.add(f), () => zoomSubs.delete(f)), () => zoom);

/** Ctrl+roda, Ctrl+= / Ctrl+- / Ctrl+0: zoom do email em vez da janela. Touchpad manda dezenas de eventos por gesto: um degrau a cada 150 ms. */
let lastWheel = 0;
function zoomKeys(e: KeyboardEvent | WheelEvent) {
  if (!e.ctrlKey && !e.metaKey) return false;
  // `instanceof WheelEvent` falha aqui: o evento nasce no realm do iframe, não no da janela principal.
  if ("deltaY" in e) {
    const now = performance.now();
    if (e.deltaY && now - lastWheel > 150) { lastWheel = now; stepReaderZoom(e.deltaY < 0 ? 1 : -1); }
    return true;
  }
  const dir = e.key === "=" || e.key === "+" ? 1 : e.key === "-" ? -1 : e.key === "0" ? 0 : null;
  if (dir === null) return false;
  stepReaderZoom(dir);
  return true;
}

export function ZoomControls() {
  const z = useReaderZoom();
  return (
    <div className="zoom" role="group" aria-label="Zoom da mensagem">
      <button type="button" className="icon-btn" onClick={() => stepReaderZoom(-1)} disabled={z === ZOOM_STEPS[0]}
        aria-label="Diminuir zoom" title="Diminuir zoom (Ctrl+-)"><MagnifyingGlassMinus size={15} weight="bold" /></button>
      <button type="button" className="zoom-pct" onClick={() => stepReaderZoom(0)} title="Voltar a 100% (Ctrl+0)">{Math.round(z * 100)}%</button>
      <button type="button" className="icon-btn" onClick={() => stepReaderZoom(1)} disabled={z === ZOOM_STEPS[ZOOM_STEPS.length - 1]}
        aria-label="Aumentar zoom" title="Aumentar zoom (Ctrl+=)"><MagnifyingGlassPlus size={15} weight="bold" /></button>
    </div>
  );
}

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
  const userZoom = useReaderZoom();
  const zoomRef = useRef(userZoom);
  zoomRef.current = userZoom;
  const refit = useRef<() => void>(() => {});
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
      // Ajusta à largura: emails de largura fixa (600–700 px) encolhem até caber, sem barra lateral; o zoom do usuário multiplica.
      const fit = () => {
        const root = doc.documentElement;
        root.style.zoom = "1";
        const natural = Math.max(root.scrollWidth, doc.body.scrollWidth);
        const avail = frame.clientWidth;
        const z = (natural > avail + 1 ? avail / natural : 1) * zoomRef.current;
        root.style.zoom = String(z);
        // Mede depois do zoom: conteúdo fluido refaz a quebra de linha e não transborda mesmo com zoom acima de 100%.
        const scrolls = root.scrollWidth > root.clientWidth + 1;
        root.style.overflowX = scrolls ? "auto" : "hidden";
        // Com zoom acima do ajuste, a barra horizontal ocupa espaço embaixo: soma a altura dela.
        setHeight(Math.min(MAX_HEIGHT, Math.ceil(root.getBoundingClientRect().height) + (scrolls ? 20 : 2)));
      };
      refit.current = fit;
      observer = new ResizeObserver(fit);
      observer.observe(frame);
      observer.observe(doc.body);
      fit();
      if (wired.has(doc)) return;
      wired.add(doc);
      doc.addEventListener("wheel", (e) => { if (zoomKeys(e)) e.preventDefault(); }, { passive: false });
      doc.addEventListener("keydown", (e) => { if (zoomKeys(e)) e.preventDefault(); });
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
  useEffect(() => refit.current(), [userZoom]);

  return (
    <iframe ref={ref} className="html-body" title="Conteúdo da mensagem" sandbox="allow-same-origin"
      srcDoc={srcDoc} style={{ height }} />
  );
}
