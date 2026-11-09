// Cartão do contato: abre ao clicar no remetente ou nos destinatários. Nome, email, copiar, novo email e "ver emails dele".
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Copy, EnvelopeSimple, MagnifyingGlass, NotePencil } from "@phosphor-icons/react";
import { toast } from "./toast";

export interface ContactState { x: number; y: number; name: string; addr: string }

/** Separa "Nome <a@b.com>" ou "a@b.com" em nome e endereço. */
export function parseContact(raw: string): { name: string; addr: string } {
  const m = raw.match(/^\s*"?([^"<]*?)"?\s*<([^>]+)>\s*$/);
  if (m) return { name: m[1].trim(), addr: m[2].trim() };
  return { name: "", addr: raw.trim() };
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const t = document.createElement("textarea");
    t.value = text;
    t.style.position = "fixed";
    t.style.opacity = "0";
    document.body.appendChild(t);
    t.select();
    document.execCommand("copy");
    t.remove();
  }
  toast("Endereço copiado", "success", { duration: 1800 });
}

export function ContactCard({ c, onClose, onNew, onSearch }: { c: ContactState; onClose: () => void; onNew: (addr: string) => void; onSearch: (addr: string) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x: c.x, y: c.y });
  const prev = useRef<Element | null>(document.activeElement);

  useLayoutEffect(() => {
    const r = box.current?.getBoundingClientRect();
    if (!r) return;
    setPos({ x: Math.max(8, Math.min(c.x, window.innerWidth - r.width - 8)), y: Math.max(8, Math.min(c.y, window.innerHeight - r.height - 8)) });
  }, [c.x, c.y]);

  useEffect(() => {
    box.current?.querySelector<HTMLElement>("button")?.focus();
    const away = (e: PointerEvent) => { if (!box.current?.contains(e.target as Node)) onClose(); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") { e.stopPropagation(); onClose(); (prev.current as HTMLElement | null)?.focus?.(); } };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", esc, true);
    return () => { window.removeEventListener("pointerdown", away, true); window.removeEventListener("keydown", esc, true); };
  }, [onClose]);

  const shown = c.name || c.addr;
  return createPortal(
    <div ref={box} className="contact-card" role="dialog" aria-label={`Contato ${shown}`} style={{ left: pos.x, top: pos.y }}>
      <div className="contact-head">
        <span className="avatar large" aria-hidden>{shown[0]?.toUpperCase()}</span>
        <div className="contact-id">
          <div className="contact-name">{shown}</div>
          {c.name && <div className="contact-addr">{c.addr}</div>}
        </div>
      </div>
      <div className="contact-actions">
        <button type="button" className="btn small" onClick={() => void copy(c.addr)}><Copy size={14} weight="bold" /> Copiar email</button>
        <button type="button" className="btn small" onClick={() => { onClose(); onNew(c.addr); }}><NotePencil size={14} weight="bold" /> Novo email</button>
        <button type="button" className="btn small" onClick={() => { onClose(); onSearch(c.addr); }}><MagnifyingGlass size={14} weight="bold" /> Ver emails</button>
      </div>
      <a className="contact-mail" href={`mailto:${c.addr}`} onClick={(e) => { e.preventDefault(); onClose(); onNew(c.addr); }}>
        <EnvelopeSimple size={13} weight="bold" aria-hidden /> {c.addr}
      </a>
    </div>,
    document.body,
  );
}
