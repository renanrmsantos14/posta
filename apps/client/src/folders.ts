// Rótulo, ícone e texto de pasta vazia de cada papel, e a árvore de pastas do usuário.
import { Archive, FolderSimple, PaperPlaneTilt, Stack, Star, Tag, Trash, Tray, Warning, type Icon } from "@phosphor-icons/react";
import type { Category, Role, UserFolder, View } from "./api";
import { parseBox, parseCat } from "./views";

export const FOLDERS: Record<Role | "starred" | "all", { label: string; icon: Icon; empty: string }> = {
  all: { label: "Todas as caixas", icon: Stack, empty: "Nenhuma mensagem nas caixas de entrada" },
  inbox: { label: "Caixa de entrada", icon: Tray, empty: "Caixa de entrada vazia" },
  starred: { label: "Com estrela", icon: Star, empty: "Nenhuma mensagem com estrela" },
  sent: { label: "Enviados", icon: PaperPlaneTilt, empty: "Nada enviado ainda" },
  archive: { label: "Arquivo", icon: Archive, empty: "Nada arquivado" },
  trash: { label: "Lixeira", icon: Trash, empty: "Nada na Lixeira" },
  junk: { label: "Spam", icon: Warning, empty: "Nenhum spam" },
};

/** Pastas da conta com a virtual "Com estrela" logo depois da Caixa de entrada. */
export const withStarred = (roles: Role[]): (Role | "starred")[] => ["inbox", "starred", ...roles.filter((r) => r !== "inbox")];

/** Pastas que mostram o contador de não lidas. */
export const SHOWS_UNREAD: Role[] = ["inbox", "archive", "junk"];

/** Visões lidas só do banco local (sem pasta no servidor para sincronizar). */
export const LOCAL_VIEWS: View[] = ["inbox", "starred", "all"];

export interface FolderNode extends UserFolder {
  /** Nome sem o prefixo `INBOX.` e sem os ancestrais que aparecem na árvore. */
  label: string;
  /** Quantos ancestrais existem na árvore. */
  depth: number;
  /** Último trecho do nome (o que o usuário digita ao renomear). */
  leaf: string;
}

const rel = (f: UserFolder) => {
  const inbox = `INBOX${f.delimiter}`;
  return f.delimiter && f.name.startsWith(inbox) ? f.name.slice(inbox.length) : f.name;
};

/** Pastas por conta, em ordem alfabética por trecho, com cada subpasta logo abaixo da pasta mãe. */
export function folderTree(list: UserFolder[]): FolderNode[] {
  const collator = new Intl.Collator("pt-BR", { sensitivity: "base", numeric: true });
  const parts = (f: UserFolder) => (f.delimiter ? rel(f).split(f.delimiter) : [rel(f)]);
  const sorted = [...list].sort((a, b) => {
    if (a.account !== b.account) return collator.compare(a.account, b.account);
    const pa = parts(a), pb = parts(b);
    for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
      const c = collator.compare(pa[i], pb[i]);
      if (c) return c;
    }
    return pa.length - pb.length;
  });
  const names = new Set(sorted.map((f) => `${f.account}\u0000${rel(f)}`));
  return sorted.map((f) => {
    const p = parts(f);
    let depth = 0;
    let from = 0;
    for (let i = 1; i < p.length; i++) {
      if (names.has(`${f.account}\u0000${p.slice(0, i).join(f.delimiter)}`)) { depth++; from = i; }
    }
    return { ...f, label: p.slice(from).join(f.delimiter), depth, leaf: p[p.length - 1] };
  });
}

/** Destino de uma mudança de pasta: um papel ou uma pasta do usuário. */
export type Dest = Role | { account: string; name: string; label: string };

/** Rótulo, ícone e texto de vazio de qualquer visão. */
export function viewInfo(view: View, folders: UserFolder[], categories: Category[]): { label: string; icon: Icon; empty: string } {
  const box = parseBox(view);
  if (box) {
    const node = folderTree(folders).find((f) => f.account === box.account && f.name === box.name);
    return { label: node?.leaf ?? box.name, icon: FolderSimple, empty: "Pasta vazia" };
  }
  const cat = parseCat(view);
  if (cat) return { label: categories.find((c) => c.id === cat)?.name ?? "Categoria", icon: Tag, empty: "Nenhuma mensagem nesta categoria" };
  return FOLDERS[view as Role | "starred" | "all"];
}

/** Pastas abertas ou fechadas à mão: chave da pasta -> aberta. Sem entrada, vale o padrão (só o primeiro nível aberto). */
export type OpenMap = Record<string, boolean>;

export const folderKey = (n: { account: string; name: string }) => `${n.account}\u0000${n.name}`;

const isChild = (p: FolderNode, c: FolderNode) => c.account === p.account && c.name.startsWith(p.name + p.delimiter);

/** Quantas pastas logo depois de `i` são descendentes dela (a árvore ordenada as deixa contíguas). */
export function descendants(nodes: FolderNode[], i: number): number {
  let n = 0;
  while (i + 1 + n < nodes.length && isChild(nodes[i], nodes[i + 1 + n])) n++;
  return n;
}

export const isFolderOpen = (n: FolderNode, open: OpenMap) => open[folderKey(n)] ?? n.depth === 0;

/** Índices das pastas visíveis: as que têm algum ancestral fechado ficam de fora. */
export function visibleFolders(nodes: FolderNode[], open: OpenMap): number[] {
  const out: number[] = [];
  for (let i = 0; i < nodes.length; i++) {
    out.push(i);
    if (!isFolderOpen(nodes[i], open)) i += descendants(nodes, i);
  }
  return out;
}

export function parseOpenMap(raw: string | undefined): OpenMap {
  try {
    const v: unknown = JSON.parse(raw ?? "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? (v as OpenMap) : {};
  } catch {
    return {};
  }
}
