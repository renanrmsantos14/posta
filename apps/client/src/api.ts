// Contrato com o núcleo Rust. No navegador (sem Tauri) usa dados de exemplo para desenvolver a UI.
import { invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";

export type Security = "tls" | "starttls" | "plaintext";

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
}

export interface MessageDetail extends MessageSummary {
  body_text: string;
  /** Reply-To, ou o remetente. */
  reply_addr: string;
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

export interface SyncResult {
  fetched: number;
  errors: string[];
  skipped: boolean;
}

export const isTauri = "__TAURI_INTERNALS__" in window;

export const api = isTauri
  ? {
      listAccounts: () => invoke<Account[]>("list_accounts"),
      addAccount: (account: NewAccount) =>
        invoke<Account>("add_imap_account", { account: { ...account, id: "" } }),
      sync: () => invoke<SyncResult>("sync_inbox"),
      list: (limit: number, offset: number) =>
        invoke<MessageSummary[]>("list_messages", { limit, offset }),
      search: (query: string) => invoke<MessageSummary[]>("search_messages", { query }),
      open: (id: number) => invoke<MessageDetail | null>("open_message", { id }),
      send: (draft: Draft) => invoke<SendReport>("send_message", { draft }),
    }
  : mockApi;

export type Api = typeof mockApi;
