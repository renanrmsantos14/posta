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

/** Servidores detectados pelo email (tabela, ISPDB, MX ou palpite). */
export interface Detected {
  /** "Gmail"; vazio quando foi só um palpite. */
  provider: string;
  source: "tabela" | "ispdb" | "mx" | "palpite";
  imap: SmtpServer;
  smtp: SmtpServer;
  /** Login a usar: email inteiro ou só a parte local. */
  username: string;
  hint: { text: string; url: string | null } | null;
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

export interface Attachment {
  name: string;
  mime: string;
  /** Bytes. */
  size: number;
  /** Posição da parte na mensagem; vale para `saveAttachment`. */
  index: number;
}

/** Arquivo anexado ao rascunho; `data` é o conteúdo em base64. */
export interface OutAttachment {
  name: string;
  mime: string;
  data: string;
}

export interface MessageDetail extends MessageSummary {
  body_text: string;
  /** HTML original; vazio quando a mensagem só tem texto. */
  body_html: string;
  /** Reply-To, ou o remetente. */
  reply_addr: string;
  attachments: Attachment[];
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
  /** Vários endereços separados por vírgula ou ponto e vírgula. */
  cc?: string;
  bcc?: string;
  attachments?: OutAttachment[];
}

/** Rascunho salvo localmente pelo Composer. */
export interface StoredDraft {
  id: string;
  account: string;
  reply_to: number | null;
  to: string;
  cc?: string;
  bcc?: string;
  subject: string;
  body: string;
  attachments?: OutAttachment[];
  /** Unix, em segundos. */
  updated: number;
}

/** Configurações: `signature:<id da conta>`, `notify` ("0" desliga as notificações) e `undo_send` (segundos: "0", "5" ou "10"; padrão 5). */
export type Settings = Record<string, string>;

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
      detectServers: (email: string) => invoke<Detected>("detect_servers", { email }),
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
      unreadCounts: (account?: string | null) => invoke<UnreadCounts>("unread_counts", { account: account ?? null }),
      open: (id: number) => invoke<MessageDetail | null>("open_message", { id }),
      saveAttachment: (id: number, index: number, open: boolean) =>
        invoke<string>("save_attachment", { id, index, open }),
      forwardAttachments: (id: number) => invoke<OutAttachment[]>("forward_attachments", { id }),
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
