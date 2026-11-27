# Triagem automática

- Nível: 2 (feature grande, sem dado real em risco: tudo local; envio testado só contra SMTP local de teste)
- Etapa: 6 de 7 provar e abrir PR (PR 1 de 4: regras)
- Próximo passo: abrir o PR1 e seguir para o PR2 (adiar) · **sozinho**
- Branch e PR: `feature/triagem-automatica` · PR1 em aberto
- Protótipo: sem protótipo (segue o padrão visual de Configurações)
- Go: 27/11/2026 por Renan (brief da thread principal)
- PROD: ainda não (o merge na `master` publica release; não fiz merge)
- Atualizado: 27/11/2026

## Achado que muda o escopo
O pedido tratava as quatro entregas como novas. Três já existem em versão simples (pacote "Produtividade", PR #60/#61):

| Entrega | Já existe | Falta (escopo real) |
|---|---|---|
| Regras | `rules.rs` + Configurações › Regras: 1 condição (remetente/assunto/para contém), 1 ação por regra, "Aplicar agora à Entrada" | várias condições, domínio, tem anexo, para/cc separados, várias ações, ordenar, editar em painel, pré-visualizar/aplicar nas existentes por regra, avisar pasta inexistente e conflito, medir 10 mil × 50 |
| Adiar | `snooze`/`wake_due`, menu com 3 horários, volta ao topo não lida | atalhos pedidos (hoje à noite, amanhã 8h, próxima segunda, data), recusar passado, fuso America/Sao_Paulo por IANA, testes de virada de dia |
| Envio agendado | tabela `scheduled`, fila com 3 tentativas, lista Agendados, relógio de 20 s | rodar ao abrir (não esperar 20 s), avisar o que saiu atrasado/falhou, teste de reinício e de SMTP local |
| Resumo "Hoje" | nada | visão nova + heurística local |

## Decisões
| Data | Decisão | Por quê | Quem decidiu |
|---|---|---|---|
| 27/11 | Estender o que existe em vez de recriar | evita duplicar `rules.rs`, `later.rs` e as telas; usuário já tem regras salvas | IA |
| 27/11 | Regras continuam em `settings.rules` (JSON), formato v2 (`conditions[]`, `actions[]`, `stop`); o formato antigo (`field/contains/action/arg`) é lido e convertido | regras salvas continuam valendo sem migração destrutiva; UI e núcleo já leem esse JSON; tabela nova não traria ganho | IA |
| 27/11 | Sem tabela nova no PR1; PR2/PR3 reaproveitam `snoozes` e `scheduled` | o esquema versionado do banco só recria o cache de mensagens; tabelas de produtividade são `IF NOT EXISTS` e sobrevivem | IA |
| 27/11 | Fuso fixo America/Sao_Paulo calculado por IANA (Intl), não por offset -3 fixo | o Brasil não tem horário de verão desde 2019, mas calcular por IANA mantém o teste de virada correto para datas antigas e se o governo voltar atrás | IA |
| 27/11 | 4 PRs saem de `master` de forma independente; conflitos em `lib.rs`/`api.ts`/`mock.ts` se resolvem no rebase após cada merge | base `master` exigida | IA |

## Tarefas e provas
| # | Tarefa | Depende de | Cenário (ação → resultado) | Prova | Nível | Resultado |
|---|---|---|---|---|---|---|
| 1.1 | Casamento de regras v2 (núcleo) | | caixa/acento, domínio, várias condições, ordem, stop, conflito, formato antigo | `cargo test -p mail-core` (13 testes de `rules`) | 2 | passou (local) |
| 1.2 | 10 mil mensagens × 50 regras | 1.1 | medir o casamento | ~145 ms debug · ~16 ms release (teste `rules::`) | 2 | passou (local) |
| 1.3 | Pasta inexistente | 1.1 | move para pasta que não existe → não move, aviso no toast | `RulesReport.missing_folders` + mock | 2 | passou (local, mock) |
| 1.4 | Tela de regras | 1.1 | criar com 2 condições, prévia "7 mensagens", salvar, Esc/clique fora pedem "Descartar alterações?" | prints `prints/pr1-*.png`, console sem erro, sem rolagem horizontal a 390 px | 2 | passou (mock no navegador); Tauri não aberto |

## Pendências e fora de escopo
- Sem IA externa no resumo (heurística local).

## Retro
-
