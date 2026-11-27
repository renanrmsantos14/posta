# Triagem automática

- Nível: 2 (feature grande, sem dado real em risco: tudo local; envio testado só contra SMTP local de teste)
- Etapa: 7 de 7 entregue (os 4 PRs mesclados em 27/11/2026; falta só o QA no app real)
- Próximo passo: QA no app Tauri real, com contas verdadeiras (o que eu não pude fazer) · **com você**
- Branch e PR: PR1 #68, adiar #69, envio agendado #71, Hoje #74, todos mesclados na `master` (a ordem gerou conflitos resolvidos nos #71 e #74; ver commits de atualização)
- Protótipo: sem protótipo (segue o padrão visual de Configurações)
- PROD: sim. Os merges publicaram releases até v0.1.46 (CI e Release verdes, Windows/Linux/APK); o app se atualiza sozinho. Nada foi provado no Tauri real
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
| 27/11 | Resumo "Hoje" = janela móvel de 24 h até agora, só caixas de entrada, todas as contas; heurística em `digest.rs` (estrela/fixada/importância alta de remetente não automático e resposta em conversa em que eu escrevi => Importante; remetente de aviso ou assunto transacional => Notificações; `List-Unsubscribe`/remetente de newsletter => Newsletters; `noreply` sem lista => Notificações; resto => Pessoas) | sem IA nem rede; reaproveita `messages.unsub`; remetente automático nunca vira Importante só por pedir "alta importância" | IA |
| 27/11 | "Sem rede" no agendado reaproveita a Caixa de saída offline | caminho único de envio, evita duplicar | IA |
| 27/11 | 4 PRs saem de `master` de forma independente; conflitos em `lib.rs`/`api.ts`/`mock.ts` se resolvem no rebase após cada merge | base `master` exigida | IA |

## Tarefas e provas
| # | Tarefa | Depende de | Cenário (ação → resultado) | Prova | Nível | Resultado |
|---|---|---|---|---|---|---|
| 1.1 | Casamento de regras v2 (núcleo) | | caixa/acento, domínio, várias condições, ordem, stop, conflito, formato antigo | `cargo test -p mail-core` (13 testes de `rules`) | 2 | passou (local) |
| 1.2 | 10 mil mensagens × 50 regras | 1.1 | medir o casamento | ~145 ms debug · ~16 ms release (teste `rules::`) | 2 | passou (local) |
| 1.3 | Pasta inexistente | 1.1 | move para pasta que não existe → não move, aviso no toast | `RulesReport.missing_folders` + mock | 2 | passou (local, mock) |
| 1.4 | Tela de regras | 1.1 | criar com 2 condições, prévia "7 mensagens", salvar, Esc/clique fora pedem "Descartar alterações?" | prints `prints/pr1-*.png`, console sem erro, sem rolagem horizontal a 390 px | 2 | passou (mock no navegador); Tauri não aberto |
| 2.x | Adiar: atalhos, recusa de passado, IANA | 1.1 | atalhos e virada de dia em America/Sao_Paulo | `when.test.mjs` + testes de `later.rs`; prints `pr2-*.png` (PR #69) | 2 | passou (local, mock) |
| 3.1 | Envio agendado: app fechado → abre | | reinício com arquivo, na hora/sem rede, falha→3ª→retry, ordem | 4 testes de `later.rs` + `tests/envio_agendado.rs` com Greenmail/SMTP local (chega ao destinatário; servidor fora do ar → tenta de novo → chega) | 2 | passou (local) |
| 3.2 | Avisos de atraso/falha e "Tentar de novo" | 3.1 | agendado vence com app fechado → toast; 3 falhas → "Falhou" | `sendReport.test.mjs` (4) + prints `pr3-*.png` (PR #71) | 2 | passou (mock); Tauri não aberto |
| 4.1 | Resumo "Hoje": agrupamento | | pessoa, estrela/fixada, resposta minha, aviso, newsletter, noreply, acento/caixa | 8 testes de `classify` em `digest.rs` | 2 | passou (local) |
| 4.2 | Resumo "Hoje": consulta | 4.1 | vazio, só 24 h, adiadas e Lixeira fora, lidas entram, várias contas | 4 testes de `Store::digest` | 2 | passou (local) |
| 4.3 | Tela "Hoje" | 4.2 | abrir, recolher grupo, abrir mensagem, voltar à Entrada | prints `pr4-*.png` (PR #74), console sem erro | 2 | passou (mock); Tauri não aberto |

## Pendências e fora de escopo
- Sem IA externa no resumo (heurística local).
- Tauri real nunca aberto (só `cargo check -p posta`): QA no app com contas de verdade é seu; nenhuma conta real foi usada nos testes (Greenmail local).
- Conflito provável na linha `"test"` do `apps/client/package.json` entre #69 e #71 (resolver no rebase após o primeiro merge).
- Horário de verão não existe mais no Brasil (desde 2019); o fuso usa IANA mesmo assim.
- Fora de escopo no "Hoje": badge de contagem na barra lateral e filtro por conta.

## Retro
-
