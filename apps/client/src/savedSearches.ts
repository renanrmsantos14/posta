// Pesquisas salvas: guardadas em `settings.searches` (JSON), como regras e respostas prontas. Sem dependências, para o teste e a UI usarem.

export interface SavedSearch {
  id: string;
  name: string;
  /** Consulta como digitada, com filtros (`de:`, `tem:anexo`...). */
  query: string;
  /** Pasta onde a busca foi feita (`inbox`, `box:...`). */
  folder: string;
  /** "all" = todas as caixas; "folder" = só a pasta. */
  scope: "folder" | "all";
  /** Conta ativa ao salvar; null = todas as contas. */
  account: string | null;
}

export function parseSavedSearches(raw: string | undefined): SavedSearch[] {
  try {
    const list: unknown = JSON.parse(raw || "[]");
    if (!Array.isArray(list)) return [];
    return list.flatMap((x) => {
      if (!x || typeof x !== "object") return [];
      const s = x as Record<string, unknown>;
      if (typeof s.id !== "string" || typeof s.name !== "string" || typeof s.query !== "string" || !s.query.trim()) return [];
      return [{
        id: s.id, name: s.name, query: s.query,
        folder: typeof s.folder === "string" && s.folder ? s.folder : "inbox",
        scope: s.scope === "all" ? "all" : "folder",
        account: typeof s.account === "string" ? s.account : null,
      } satisfies SavedSearch];
    });
  } catch {
    return [];
  }
}

export const serializeSavedSearches = (list: SavedSearch[]) => JSON.stringify(list);

/** Nome sugerido: a própria consulta, cortada. */
export const suggestName = (query: string) => query.trim().slice(0, 40);

export const newSearchId = () => `s${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
