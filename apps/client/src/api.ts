// Contrato com o núcleo Rust. No navegador (sem Tauri) usa dados de exemplo para desenvolver a UI.
import { invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";

export type Security = "tls" | "starttls" | "plaintext";
export type Provider = "google" | "microsoft";

export interface SmtpServer {
  host: string;
  port: number;
  security: Security;
}

export interface Account {
  id: string;
  host: string;
  port: number;
  security: Security;
  username: string;
  smtp?: SmtpServer | null;
  /** Ausente em contas antigas = senha. */
  auth?: "password" | Provider;
}

export interface NewAccount extends Omit<Account, "id"> {
  password: string;
}

export interface MessageSummary {
  id: number;
  account: string;
  uid: number;
  from_name: string;
  from_addr: string;
  subject: string;
  /** Unix, em segundos. */
  date: number;
  snippet: string;
  seen: boolean;
  /** Estrela (`\Flagged`). */
  flagged: boolean;
  /** Papel da pasta onde a mensagem está. */
  role: Role | null;
}

export interface MessageDetail extends MessageSummary {
  body_text: string;
  /** HTML original; vazio quando a mensagem só tem texto. */
  body_html: string;
  /** Reply-To, ou o remetente. */
  reply_addr: string;
  /** Endereços de To e Cc, separados por vírgula (para responder a todos). */
  to_addrs: string;
  cc_addrs: string;
}

export interface Draft {
  account: string;
  /** Id local da mensagem respondida. */
  reply_to: number | null;
  to: string;
  subject: string;
  body: string;
}

export interface SendReport {
  saved_to_sent: boolean;
  warning: string | null;
}

export type Role = "inbox" | "sent" | "archive" | "trash" | "junk";
/** O que a barra lateral mostra: pastas reais e a pasta virtual "Com estrela". */
export type View = Role | "starred";
/** Não lidas por pasta. */
export type UnreadCounts = Partial<Record<Role, number>>;

export interface SyncResult {
  fetched: number;
  /** Mensagens no servidor nas pastas sincronizadas. */
  total: number;
  errors: string[];
  skipped: boolean;
}

export const isTauri = "__TAURI_INTERNALS__" in window;

export const api = isTauri
  ? {
      listAccounts: () => invoke<Account[]>("list_accounts"),
      addAccount: (account: NewAccount) =>
        invoke<Account>("add_imap_account", { account: { ...account, id: "" } }),
      oauthProviders: () => invoke<Provider[]>("oauth_providers"),
      oauthLogin: (provider: Provider) => invoke<Account>("oauth_login", { provider }),
      folders: () => invoke<Role[]>("list_folders"),
      sync: () => invoke<SyncResult>("sync_inbox"),
      syncFolder: (role: Role, keep: number) => invoke<SyncResult>("sync_folder", { role, keep }),
      list: (view: View, limit: number, offset: number) =>
        view === "starred"
          ? invoke<MessageSummary[]>("list_starred", { limit, offset })
          : invoke<MessageSummary[]>("list_messages", { role: view, limit, offset }),
      search: (query: string, view: View) =>
        view === "starred"
          ? invoke<MessageSummary[]>("search_starred", { query })
          : invoke<MessageSummary[]>("search_messages", { query, role: view }),
      move: (id: number, role: Role) => invoke<void>("move_message", { id, role }),
      moveMany: (ids: number[], role: Role) => invoke<void>("move_messages", { ids, role }),
      deleteForever: (id: number) => invoke<void>("delete_forever", { id }),
      deleteForeverMany: (ids: number[]) => invoke<void>("delete_forever_many", { ids }),
      setSeen: (ids: number[], seen: boolean) => invoke<void>("set_seen_many", { ids, seen }),
      setFlagged: (ids: number[], flagged: boolean) => invoke<void>("set_flagged_many", { ids, flagged }),
      unreadCounts: () => invoke<UnreadCounts>("unread_counts"),
      open: (id: number) => invoke<MessageDetail | null>("open_message", { id }),
      send: (draft: Draft) => invoke<SendReport>("send_message", { draft }),
    }
  : mockApi;

export type Api = typeof mockApi;
