// Rótulo, ícone e texto de pasta vazia de cada papel.
import { Archive, PaperPlaneTilt, Trash, Tray, Warning, type Icon } from "@phosphor-icons/react";
import type { Role } from "./api";

export const FOLDERS: Record<Role, { label: string; icon: Icon; empty: string }> = {
  inbox: { label: "Caixa de entrada", icon: Tray, empty: "Caixa de entrada vazia" },
  sent: { label: "Enviados", icon: PaperPlaneTilt, empty: "Nada enviado ainda" },
  archive: { label: "Arquivo", icon: Archive, empty: "Nada arquivado" },
  trash: { label: "Lixeira", icon: Trash, empty: "Nada na Lixeira" },
  junk: { label: "Spam", icon: Warning, empty: "Nenhum spam" },
};
