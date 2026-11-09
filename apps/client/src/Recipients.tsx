// Campo de destinatários (Para/Cc/Cco): cada endereço vira um chip removível e o que se digita sugere
// contatos já vistos nas mensagens. O valor continua sendo a string "a@x, Nome <b@y>" que o envio já entende.
import { useEffect, useId, useRef, useState } from "react";
import { X } from "@phosphor-icons/react";
import { api, type Contact } from "./api";

interface Props {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  invalid?: boolean;
  inputRef?: React.Ref<HTMLInputElement>;
  /** Endereços das minhas contas: não são sugeridos. */
  own?: string[];
  trailing?: React.ReactNode;
}

const split = (v: string) => v.split(/[,;]/).map((s) => s.trim()).filter(Boolean);
const bare = (token: string) => (/<([^>]+)>\s*$/.exec(token)?.[1] ?? token).trim().toLowerCase();

/** Nome de exibição do chip: o nome, se houver; senão o endereço. */
function labelOf(token: string) {
  const m = /^"?(.*?)"?\s*<([^>]+)>$/.exec(token);
  return m && m[1] ? m[1] : (m?.[2] ?? token);
}

/** Como o contato entra no campo; nomes com vírgula, ponto e vírgula ou aspas não cabem na lista separada por vírgula. */
const tokenOf = (c: Contact) => (c.name && !/[,;"<>]/.test(c.name) ? `${c.name} <${c.addr}>` : c.addr);

const valid = (token: string) => /[^\s<@]+@[^\s>@]+\.[^\s>@]+/.test(token);

export function Recipients({ label, value, onChange, placeholder, invalid, inputRef, own = [], trailing }: Props) {
  const tokens = split(value);
  const [text, setText] = useState("");
  const [items, setItems] = useState<Contact[]>([]);
  const [active, setActive] = useState(0);
  const [open, setOpen] = useState(false);
  const listId = useId();
  const seq = useRef(0);
  const inner = useRef<HTMLInputElement>(null);
  const taken = tokens.map(bare);

  useEffect(() => {
    const q = text.trim();
    if (!q) { setItems([]); setOpen(false); return; }
    const mine = ++seq.current;
    const t = setTimeout(() => {
      api.suggestContacts(q, 8).then((list) => {
        if (mine !== seq.current) return;
        const skip = new Set([...own.map((a) => a.toLowerCase()), ...taken]);
        const shown = list.filter((c) => !skip.has(c.addr.toLowerCase())).slice(0, 6);
        setItems(shown);
        setActive(0);
        setOpen(shown.length > 0);
      }).catch(() => { if (mine === seq.current) setOpen(false); });
    }, 80);
    return () => clearTimeout(t);
  }, [text]);

  const add = (more: string[]) => {
    const fresh = more.map((s) => s.trim()).filter((s) => s && !taken.includes(bare(s)));
    if (fresh.length) onChange([...tokens, ...fresh].join(", "));
    setText("");
    setOpen(false);
  };
  const remove = (i: number) => onChange(tokens.filter((_, j) => j !== i).join(", "));

  /** Digitou ou colou vírgula/ponto e vírgula: tudo antes vira chip, o resto continua no campo. */
  const onText = (v: string) => {
    const parts = v.split(/[,;]/);
    if (parts.length === 1) return setText(v);
    const rest = parts.pop() ?? "";
    add(parts);
    setText(rest.trimStart());
  };

  const choose = (c: Contact) => { add([tokenOf(c)]); inner.current?.focus(); };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const showing = open && items.length > 0;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (!showing) return;
      e.preventDefault();
      setActive((a) => (a + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length);
    } else if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && !e.ctrlKey && !e.metaKey && text.trim()) {
      if (showing) { e.preventDefault(); choose(items[active]); }
      else if (e.key === "Enter") { e.preventDefault(); add([text]); }
      else add([text]);
    } else if (e.key === "Escape" && showing) {
      e.stopPropagation();
      setOpen(false);
    } else if (e.key === "Backspace" && !text && tokens.length) {
      remove(tokens.length - 1);
    }
  };

  return (
    <div className="line recip" data-invalid={invalid ? true : undefined} onClick={() => inner.current?.focus()}>
      <span>{label}</span>
      <div className="recip-field">
        {tokens.map((t, i) => (
          <span key={`${t}-${i}`} className="rcpt" data-bad={!valid(t) || undefined} title={bare(t)}>
            <span className="rcpt-name">{labelOf(t)}</span>
            <button type="button" className="chip-x" aria-label={`Remover ${labelOf(t)}`} tabIndex={-1}
              onClick={(e) => { e.stopPropagation(); remove(i); }}>
              <X size={12} weight="bold" />
            </button>
          </span>
        ))}
        <input
          ref={(el) => {
            inner.current = el;
            if (typeof inputRef === "function") inputRef(el);
            else if (inputRef) (inputRef as React.RefObject<HTMLInputElement | null>).current = el;
          }}
          type="text" inputMode="email" autoComplete="off" value={text} placeholder={tokens.length ? "" : placeholder}
          aria-label={label} aria-invalid={invalid} role="combobox" aria-autocomplete="list"
          aria-expanded={open} aria-controls={listId} aria-activedescendant={open ? `${listId}-${active}` : undefined}
          onChange={(e) => onText(e.target.value)} onKeyDown={onKey}
          onBlur={() => { if (text.trim()) add([text]); setOpen(false); }}
        />
        {open && (
          <ul id={listId} className="suggest-list" role="listbox" aria-label={`Sugestões para ${label}`}>
            {items.map((c, i) => (
              <li key={c.addr} id={`${listId}-${i}`} role="option" aria-selected={i === active}
                className="suggest" data-active={i === active || undefined}
                onMouseDown={(e) => { e.preventDefault(); choose(c); }} onMouseMove={() => setActive(i)}>
                <span className="suggest-name">{c.name || c.addr}</span>
                {c.name && <span className="suggest-addr">{c.addr}</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
      {trailing}
    </div>
  );
}
