// Rótulo, ícone e texto de pasta vazia de cada papel.
import { Archive, PaperPlaneTilt, Stack, Star, Trash, Tray, Warning, type Icon } from "@phosphor-icons/react";
import type { Role, View } from "./api";

export const FOLDERS: Record<View, { label: string; icon: Icon; empty: string }> = {
  all: { label: "Todas as caixas", icon: Stack, empty: "Nenhuma mensagem nas caixas de entrada" },
  inbox: { label: "Caixa de entrada", icon: Tray, empty: "Caixa de entrada vazia" },
  starred: { label: "Com estrela", icon: Star, empty: "Nenhuma mensagem com estrela" },
  sent: { label: "Enviados", icon: PaperPlaneTilt, empty: "Nada enviado ainda" },
  archive: { label: "Arquivo", icon: Archive, empty: "Nada arquivado" },
  trash: { label: "Lixeira", icon: Trash, empty: "Nada na Lixeira" },
  junk: { label: "Spam", icon: Warning, empty: "Nenhum spam" },
};

/** Pastas da conta com a virtual "Com estrela" logo depois da Caixa de entrada. */
export const withStarred = (roles: Role[]): View[] => ["inbox", "starred", ...roles.filter((r) => r !== "inbox")];

/** Pastas que mostram o contador de não lidas. */
export const SHOWS_UNREAD: Role[] = ["inbox", "archive", "junk"];

/** Visões lidas só do banco local (sem pasta no servidor para sincronizar). */
export const LOCAL_VIEWS: View[] = ["inbox", "starred", "all"];
