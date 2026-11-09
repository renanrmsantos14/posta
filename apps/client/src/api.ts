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
}

export interface MessageDetail extends MessageSummary {
  body_text: string;
  /** HTML original; vazio quando a mensagem só tem texto. */
  body_html: string;
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

/** Rascunho salvo localmente pelo Composer. */
export interface StoredDraft {
  id: string;
  account: string;
  reply_to: number | null;
  to: string;
  subject: string;
  body: string;
  /** Unix, em segundos. */
  updated: number;
}

/** Configurações: `signature:<id da conta>` e `notify` ("0" desliga as notificações). */
export type Settings = Record<string, string>;

export interface SendReport {
  saved_to_sent: boolean;
  warning: string | null;
}

export type Role = "inbox" | "sent" | "archive" | "trash" | "junk";

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
      list: (role: Role, limit: number, offset: number) =>
        invoke<MessageSummary[]>("list_messages", { role, limit, offset }),
      search: (query: string, role: Role) => invoke<MessageSummary[]>("search_messages", { query, role }),
      move: (id: number, role: Role) => invoke<void>("move_message", { id, role }),
      deleteForever: (id: number) => invoke<void>("delete_forever", { id }),
      open: (id: number) => invoke<MessageDetail | null>("open_message", { id }),
      send: (draft: Draft) => invoke<SendReport>("send_message", { draft }),
      removeAccount: (id: string) => invoke<void>("remove_account", { id }),
      saveDraft: (draft: Omit<StoredDraft, "updated">) => invoke<void>("save_draft", { draft }),
      listDrafts: () => invoke<StoredDraft[]>("list_drafts"),
      deleteDraft: (id: string) => invoke<void>("delete_draft", { id }),
      settings: () => invoke<Settings>("get_settings"),
      setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
      setFocused: (focused: boolean) => invoke<void>("set_focused", { focused }),
      requestNotifyPermission: () => invoke<boolean>("request_notify_permission"),
    }
  : mockApi;

export type Api = typeof mockApi;
