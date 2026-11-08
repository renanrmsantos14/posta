# Arquivar, excluir e pastas — desenho

Data: 08/11/2026. Aprovado no chat (abordagem A).

## Objetivo

- Arquivar (`e`) e excluir (`#`) a mensagem aberta, com 4 s para desfazer.
- O menu lateral lista as pastas padrão de cada conta: Caixa de entrada, Enviados, Arquivo, Lixeira, Spam.
- Fora do escopo: pastas e marcadores criados pelo usuário, mover para pasta qualquer, arrastar e soltar, seleção múltipla.

## Decisões

| Tema | Decisão |
| --- | --- |
| Excluir | Move para a Lixeira. Dentro da Lixeira, excluir apaga de vez, com confirmação e sem desfazer. |
| Sincronização | Caixa de entrada completa, como hoje. Demais pastas sincronizam ao abrir, mantendo as 1.000 mais recentes; rolar até o fim traz mais 500. |
| Desfazer | Abordagem A: a mensagem some da lista na hora; o comando vai ao servidor só quando os 4 s acabam. Desfazer cancela sem rede. Fechar o app nesses 4 s descarta a ação (mesmo comportamento do envio). |
| Gmail | "Arquivo" é a pasta `\All` (Todos os e-mails). Arquivar = mover da Caixa de entrada para ela, o que remove o marcador Inbox. |

## Pastas (mail-core)

- Novo `folders.rs`: `discover(session) -> Folders`, com papéis `Inbox`, `Sent`, `Archive`, `Trash`, `Junk`.
  - Fonte principal: atributos SPECIAL-USE do `LIST` (`\Sent`, `\Archive`, `\All`, `\Trash`, `\Junk`). `\Archive` tem prioridade sobre `\All`.
  - Sem atributo: nomes comuns, sem diferenciar maiúsculas (`Sent`, `Sent Items`, `Enviados`, `Itens Enviados`, `Archive`, `Arquivo`, `Trash`, `Deleted Items`, `Lixeira`, `Itens Excluídos`, `Junk`, `Spam`, `Lixo Eletrônico`), também sob o prefixo `INBOX.`.
  - Arquivo ausente: cria `Archive` (com o delimitador do servidor quando houver prefixo `INBOX.`) e assina a pasta.
  - Outros papéis ausentes ficam fora do menu daquela conta.
- A descoberta de Enviados que hoje está em `send.rs` (`sent_folder`, `SENT_NAMES`) migra para `folders.rs`; `send.rs` passa a usá-la. A regra do Gmail (não copiar para Enviados) continua.
- Store: nova tabela `folders(account, role, name, PRIMARY KEY(account, role))`, com `set_folders` e `folders(account)`. A descoberta roda ao adicionar a conta e no início de cada sincronização da Caixa de entrada.

## Sincronização

- `sync_mailbox(acc, store, mailbox, keep: Option<usize>)`:
  - `None`: comportamento atual (tudo).
  - `Some(n)`: busca UIDs e flags de toda a pasta, mas só baixa o corpo das `n` mais recentes que faltam localmente. Mensagens locais mais antigas que a janela continuam; as removidas no servidor somem.
- Comando `sync_folder(account, role, keep)`: a UI chama ao abrir uma pasta (`keep = 1000`) e ao rolar até o fim (`keep` atual + 500). Caixa de entrada continua em `sync_inbox`.
- Uma sincronização por vez por app, como hoje (`syncing`); a chamada pulada retorna `skipped`.

## Mover e excluir

- `imap_sync::move_message(acc, from, uid, to)`: `UID MOVE` quando o servidor anuncia `MOVE`; senão `UID COPY` + `+FLAGS.SILENT (\Deleted)` + `UID EXPUNGE` (ou `EXPUNGE` sem `UIDPLUS`).
- `imap_sync::delete_forever(acc, mailbox, uid)`: `\Deleted` + expunge.
- Store: `remove(id)` apaga a linha local (e o FTS). A pasta de destino traz a mensagem de volta na próxima sincronização dela.
- Comandos Tauri: `move_message(id, role)` e `delete_forever(id)`. Leem conta, pasta e UID da linha local; apagam a linha só depois de o servidor confirmar.
- `list_messages` e `search_messages` recebem o `role` da pasta aberta e juntam todas as contas (como a Caixa de entrada já faz); a busca filtra pela pasta.

## Interface

- Menu lateral: uma entrada por papel, unificada entre contas (aparece se ao menos uma conta tem a pasta), com ícones Phosphor (Tray, PaperPlaneTilt, Archive, Trash, Warning). Pasta ativa marcada. No celular (sem menu lateral), as mesmas pastas numa faixa de abas abaixo da barra superior.
- Mensagem aberta: botões Arquivar e Excluir na barra; atalhos `e` e `#` (Shift+3) quando nenhum campo de texto tem foco. Na Lixeira, o botão vira "Excluir de vez"; em Arquivo, o botão Arquivar some.
- Fluxo:
  1. Ação → a mensagem sai da lista e a seleção vai para a próxima (ou anterior, se era a última).
  2. Aviso `Arquivada · Desfazer` / `Movida para a Lixeira · Desfazer`, com barra de 4 s.
  3. Desfazer → a mensagem volta para a mesma posição e é selecionada de novo.
  4. Fim dos 4 s → chama o servidor. Falha → a mensagem volta à lista e aparece o aviso de erro com o motivo.
- "Excluir de vez": confirmação no painel da mensagem ("Excluir de vez?" — "Cancelar" / "Excluir"); sem desfazer.
- Pasta vazia: texto simples ("Nada na Lixeira"). Carregando pasta pela primeira vez: texto "Carregando…".
- Modo mock (navegador): pastas fixas e mover em memória, para prints e testes de UI.

## Erros

- Pasta de destino não descoberta: botão desabilitado com dica ("Esta conta não tem Lixeira").
- Servidor recusa ou cai: a mensagem volta para a lista e o aviso de erro mostra o motivo; nada é apagado localmente.
- UID inexistente no servidor (já movida em outro cliente): trata como sucesso e apaga a linha local.

## Testes

- `cargo test` (unitários): escolha de papéis a partir de listas de `LIST` (SPECIAL-USE, nomes comuns, prefixo `INBOX.`, Gmail `\All`); janela `keep` do sync; `remove` no store e no FTS.
- `pnpm test:imap` (Greenmail): descobrir pastas, criar Arquivo ausente, mover e encontrar a mensagem no destino após sync, apagar de vez, enviar e achar a cópia em Enviados.
- UI: prints desktop e celular (pasta aberta, aviso de desfazer, confirmação "Excluir de vez"); auditoria UX rápida.
