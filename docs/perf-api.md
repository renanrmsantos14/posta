# API de desempenho (backend)

Resumo do que mudou para a UI e para quem usa o `mail-core`. Todos os comandos antigos continuam funcionando.

## Comando novo

`list_messages_page(folder: Role, cursor?: string, limit: number) -> { items: MessageSummary[], next: string | null }`

- Paginação por cursor (keyset), custo O(limit) em qualquer profundidade; mensagens que chegam no topo não deslocam a página seguinte.
- Primeira página: `cursor` ausente. Próximas: passe o `next` recebido. `next = null` no fim.
- Ordem: fixadas primeiro, depois `visible_date` (maior de data e fim do adiamento) e `id`, decrescentes. Adiadas ficam fora.
- O cursor é opaco para a UI (formato interno `fixada:data:id`). Cursor inválido devolve erro.
- No `mail-core`: `Store::list_folder_page` e `Store::list_box_page` (pasta de usuário).

## Eventos

| Evento | Carga | Quando |
|---|---|---|
| `mail-changed` | número de mensagens novas | igual a antes: fim de IDLE/sync, agendador |
| `mail-delta` | `{ account, mailbox, inserted: number[], changed: number[], removed: number[] }` | depois de cada sync, no IDLE e na pré-busca de corpos |

Os ids são os ids locais (`MessageSummary.id`). `inserted` = chegaram, `changed` = flags ou corpo mudaram, `removed` = sumiram do servidor. O evento só sai quando há alguma mudança. A UI pode aplicar o delta sem recarregar a lista.

## Sincronização

- `SyncReport { fetched, total, mode }`; `mode` informa como a pasta foi reconciliada:
  - `qresync`: servidor anuncia QRESYNC; só vem o que mudou e as exclusões (VANISHED).
  - `condstore`: `CHANGEDSINCE` para flags; exclusões conferidas na janela recente.
  - `uidnext`: sem extensões; novos por UIDNEXT e flags da janela recente (500 mensagens).
  - `full`: primeira vez ou UIDVALIDITY nova.
- Por pasta ficam gravados `uid_validity`, `highest_modseq`, `uid_next` e se a cópia está completa (`SyncState`).
- Header-first: baixa `BODY.PEEK[]<0.8192>` mais BODYSTRUCTURE. Mensagem de até 8 KiB chega inteira; as maiores ficam só com cabeçalho, prévia e anexos (`body_loaded = false`). Servidores que não entendem leitura parcial (ex.: GreenMail) caem sozinhos para mensagem inteira.
- Corpo sob demanda: `open_message` baixa o corpo quando falta e grava no SQLite; a partir daí abre sem rede. Sem rede, abre com a prévia.
- Pré-busca: depois de cada ciclo da Entrada, até 200 corpos recentes que faltam são baixados em segundo plano (uma pré-busca por conta por vez).
- `set_seen` segue pela fila offline (`run_op`): não bloqueia a abertura.

## Sessão persistente

`mail_core::Hub` guarda uma sessão IMAP por conta e uso (`Kind::Cmd`, `Kind::Idle`). `Hub::with` reaproveita a conexão e, em erro de rede, descarta, reconecta e repete (até 3 tentativas, espera 0, 300 ms e 1 s). O ciclo de IDLE usa a sessão `Idle` e sincroniza na sessão `Cmd`, sem novo login: o email novo chega ao banco em menos de 1 s depois do aviso (teste `sessao_persistente_idle_entrega_em_menos_de_1s_e_corpo_sob_demanda`).

Todo o resto também passa pela sessão `Cmd`: `fetch_raw_on`, `fetch_to_on`, `set_flag_on`, `move_messages_on`, `delete_forever_on`, `outbox::apply_op_via` e `outbox::flush_via` (as versões sem `_on`/`_via` abrem uma conexão avulsa com `Live::once`). Duas operações na mesma conta esperam uma pela outra no mesmo slot; um sync longo segura o slot `Cmd`. `Hub::drop_account` (chamado ao salvar ou remover a conta) descarta a sessão guardada, mas não fecha um IDLE em uso.

Corpo sob demanda: `open_message` sem corpo no cache baixa pela sessão; sem rede devolve o erro "Sem conexão: o corpo desta mensagem ainda não foi baixado." (a mensagem continua não lida e `body_loaded` fica falso), nunca uma tela vazia.

## Banco (SQLite)

- `user_version` continua 5. A migração é idempotente e preenche o que falta (teste com o schema do master e dados reais).
- Coluna gerada `visible_date`, `pinned` e índice `messages_by_top (account, mailbox, pinned DESC, visible_date DESC, id DESC)` para a primeira página.
- Fios (`thread_id`) recalculados só nos componentes tocados pela inserção (`insert_messages` deixou de ser quadrático); tabela `msg_refs`.
- Busca: FTS5 com prefixo (`prefix='2'`); incluem corpos que chegam depois. Contatos em tabela própria e indexada (`suggest_contacts`).
- `unread_counts` por índice parcial.
- PRAGMAs: WAL, `synchronous=NORMAL`, `cache_size` e `mmap_size` maiores, `wal_autocheckpoint` alto, mais `Store::checkpoint()` após sincronizações grandes. `Store::open` usa caminho rápido por `user_version`/`application_id` e não reexecuta o schema.

## Medições (50 mil mensagens, `tests/perf_bench.rs`)

Insert de 50k: ~10,2 s (meta ~10 s; FTS com `prefix='2'` em vez de `'2 3'` poupou ~2,5 s; o resto é custo por linha de índices e FTS, sem tentar remover `messages_by_date`/`messages_by_box`, que a listagem usa). Veja a tabela no PR. Reproduzir: `PERF_N=50000 PERF_DIR=<pasta> cargo test -p mail-core --release --test perf_bench -- --nocapture` (sem `PERF_N` o teste é ignorado).
