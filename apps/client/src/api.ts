// Contrato com o núcleo Rust. No navegador (sem Tauri) usa dados de exemplo para desenvolver a UI.
import { invoke } from "@tauri-apps/api/core";
import { mockApi } from "./mock";
import { parseBox, parseCat } from "./views";

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
  /** Nome de exibição no From dos envios. */
  name?: string | null;
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
  /** Fio (conversa) da conta; 0 = sem fio. */
  thread_id: number;
  /** Mensagens do fio, em qualquer pasta. */
  thread_count: number;
  /** Id da categoria colorida (local), se houver. */
  category?: string | null;
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
  /** Texto simples (parte `text/plain`); no Composer é derivado de `html`. */
  body: string;
  /** Corpo formatado; ausente = só texto. */
  html?: string;
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
  /** Vazio em rascunhos só de texto (versões antigas). */
  html?: string;
  /** Unix, em segundos. */
  updated: number;
}

/** Pessoa já vista nas mensagens, para autocompletar destinatários. */
export interface Contact {
  name: string;
  addr: string;
  count: number;
}

/** Configurações: `signature:<id da conta>`, `offlineN` (corpos baixados para leitura offline, padrão 200), `notify` ("0" desliga as notificações) e `group` ("0" desliga o agrupamento por conversa). */
export type Settings = Record<string, string>;

export interface SendReport {
  saved_to_sent: boolean;
  warning: string | null;
  /** Sem rede: foi para a Caixa de saída e sai quando a conexão voltar. */
  queued: boolean;
}

export type Role = "inbox" | "sent" | "archive" | "trash" | "junk";
/** O que a barra lateral mostra: pastas reais, as virtuais "Com estrela" e "Todas as caixas" (Entrada de todas as contas), pastas do usuário e categorias. */
export type View = Role | "starred" | "all" | `box:${string}` | `cat:${string}`;

/** Pasta criada pelo usuário; `name` é o nome completo no servidor. */
export interface UserFolder {
  account: string;
  name: string;
  delimiter: string;
}

/** Categoria colorida; `color` é uma chave da paleta (ver Categories.tsx). */
export interface Category {
  id: string;
  name: string;
  color: string;
}
/** Não lidas por pasta. */
export type UnreadCounts = Partial<Record<Role, number>>;

export interface SyncResult {
  fetched: number;
  /** Mensagens no servidor nas pastas sincronizadas. */
  total: number;
  errors: string[];
  skipped: boolean;
  /** Sem rede: nada foi sincronizado; o app segue com o que está salvo. */
  offline: boolean;
}

/** Estado da fila offline: servidor alcançável, ações à espera dele e mensagens na Caixa de saída. */
export interface QueueStatus {
  online: boolean;
  pending: number;
  outbox: number;
}

/** Mensagem na Caixa de saída; `error` não vazio = o servidor recusou e ela ficou parada. */
export interface OutboxItem {
  id: number;
  account: string;
  to: string;
  subject: string;
  /** Unix, em segundos. */
  created: number;
  error: string;
}

export interface FlushReport {
  applied: number;
  discarded: number;
  sent: number;
  failed: number;
  offline: boolean;
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
      syncFolder: (view: View, keep: number) => {
        const box = parseBox(view);
        return box
          ? invoke<SyncResult>("sync_user_folder", { account: box.account, folder: box.name, keep })
          : invoke<SyncResult>("sync_folder", { role: view, keep });
      },
      list: (view: View, limit: number, offset: number) => {
        const box = parseBox(view);
        const cat = parseCat(view);
        if (box) return invoke<MessageSummary[]>("list_user_folder", { account: box.account, folder: box.name, limit, offset });
        if (cat) return invoke<MessageSummary[]>("list_category", { category: cat, limit, offset });
        return view === "starred"
          ? invoke<MessageSummary[]>("list_starred", { limit, offset })
          : invoke<MessageSummary[]>("list_messages", { role: view === "all" ? "inbox" : view, limit, offset });
      },
      search: (query: string, view: View) => {
        const box = parseBox(view);
        const cat = parseCat(view);
        if (box) return invoke<MessageSummary[]>("search_user_folder", { account: box.account, folder: box.name, query });
        if (cat) return invoke<MessageSummary[]>("search_category", { category: cat, query });
        return view === "starred"
          ? invoke<MessageSummary[]>("search_starred", { query })
          : invoke<MessageSummary[]>("search_messages", { query, role: view === "all" ? "inbox" : view });
      },
      thread: (id: number) => invoke<MessageSummary[]>("thread_messages", { id }),
      userFolders: () => invoke<UserFolder[]>("list_user_folders"),
      createFolder: (account: string, parent: string | null, name: string) => invoke<UserFolder[]>("create_folder", { account, parent, name }),
      renameFolder: (account: string, folder: string, name: string) => invoke<UserFolder[]>("rename_folder", { account, folder, name }),
      deleteFolder: (account: string, folder: string) => invoke<UserFolder[]>("delete_folder", { account, folder }),
      countFolder: (account: string, folder: string) => invoke<number>("count_folder", { account, folder }),
      moveToFolder: (ids: number[], account: string, folder: string) => invoke<number>("move_to_folder", { ids, account, folder }),
      setCategory: (ids: number[], category: string | null) => invoke<void>("set_category", { ids, category }),
      updateAccount: (account: Account, password: string | null) => invoke<Account>("update_account", { account, password }),
      move: (id: number, role: Role) => invoke<void>("move_message", { id, role }),
      moveMany: (ids: number[], role: Role) => invoke<void>("move_messages", { ids, role }),
      deleteForever: (id: number) => invoke<void>("delete_forever", { id }),
      deleteForeverMany: (ids: number[]) => invoke<void>("delete_forever_many", { ids }),
      setSeen: (ids: number[], seen: boolean) => invoke<void>("set_seen_many", { ids, seen }),
      setFlagged: (ids: number[], flagged: boolean) => invoke<void>("set_flagged_many", { ids, flagged }),
      unreadCounts: () => invoke<UnreadCounts>("unread_counts"),
      open: (id: number) => invoke<MessageDetail | null>("open_message", { id }),
      saveAttachment: (id: number, index: number, open: boolean) =>
        invoke<string>("save_attachment", { id, index, open }),
      forwardAttachments: (id: number) => invoke<OutAttachment[]>("forward_attachments", { id }),
      send: (draft: Draft) => invoke<SendReport>("send_message", { draft }),
      removeAccount: (id: string) => invoke<void>("remove_account", { id }),
      saveDraft: (draft: Omit<StoredDraft, "updated">) => invoke<void>("save_draft", { draft }),
      listDrafts: () => invoke<StoredDraft[]>("list_drafts"),
      deleteDraft: (id: string) => invoke<void>("delete_draft", { id }),
      suggestContacts: (query: string, limit: number) => invoke<Contact[]>("suggest_contacts", { query, limit }),
      settings: () => invoke<Settings>("get_settings"),
      setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
      setFocused: (focused: boolean) => invoke<void>("set_focused", { focused }),
      requestNotifyPermission: () => invoke<boolean>("request_notify_permission"),
      queueStatus: () => invoke<QueueStatus>("queue_status"),
      flushQueue: () => invoke<FlushReport>("flush_queue"),
      listOutbox: () => invoke<OutboxItem[]>("list_outbox"),
      discardOutbox: (id: number) => invoke<void>("discard_outbox", { id }),
      retryOutbox: (id: number) => invoke<FlushReport>("retry_outbox", { id }),
    }
  : mockApi;

export type Api = typeof mockApi;
