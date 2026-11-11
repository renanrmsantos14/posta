# Auditoria Outlook × Posta (2026-11-11)

Método: lista de recursos do Outlook (documentação pública da Microsoft e guias de uso) comparada com o código do Posta (`README.md`, `apps/client/src`, `crates/mail-core`). Não houve engenharia reversa de binário: o Outlook é fechado e os recursos são públicos, então o REA não acrescenta nada. "Ausente" = busca no código sem resultado; itens marcados (?) precisam de confirmação antes de implementar.

## Já existe no Posta
Leitura/lista: conversas, Foco × Outros (heurística local), fixar, estrela (`\Flagged`), lida/não lida, categorias coloridas, adiar, seleção múltipla, arrastar e soltar, deslizar (swipe), densidade, temas, pull-to-refresh, Todas as contas.
Escrita: responder/responder a todos/encaminhar, Cc/Bcc, anexos, rascunhos automáticos, assinatura por conta, aliases, respostas prontas, agendar envio, desfazer envio (4 s), imagens inline, HTML isolado em iframe.
Automação: regras (remetente/assunto/destinatário → mover, ler, estrela, categoria, arquivar), resposta de férias, notificações, IDLE.
Outros: busca com filtros (`de:`, `para:`, `assunto:`, `tem:anexo`, `é:não-lida`, `depois:`, `antes:`), barra de comandos, atalhos, imprimir, salvar .eml, pastas (criar/mover), OAuth Google/Microsoft, modo offline, atualização automática, Android.

## Falta (ordenado por valor ÷ esforço)

| # | Recurso do Outlook | Esforço | Observação |
|---|---|---|---|
| 1 | Cancelar inscrição (`List-Unsubscribe`, 1 clique) | P | Só o cabeçalho é lido para evitar autoresposta de férias; sem botão |
| 2 | Criar regra a partir de uma mensagem ("sempre mover de este remetente") | P | Reaproveita o motor de regras |
| 3 | Regras mais ricas: condições (tem anexo, corpo, Cc, importância), múltiplas ações, "parar de processar", ordem, executar nas existentes | M | Hoje 3 campos × 5 ações |
| 4 | Pesquisas salvas / pastas de busca | P | Usa a busca atual |
| 5 | Acompanhamento: sinalizar com data e lembrete ("Para fazer") | M | Estrela hoje é só liga/desliga |
| 6 | Importância alta/baixa (envio e exibição, `Importance`/`X-Priority`) | P | Ausente |
| 7 | Silenciar conversa (Ignorar) (?) e bloquear remetente (?) | P | `store.rs` cita bloqueio; confirmar se há UI |
| 8 | Confirmação de leitura e de entrega (`Disposition-Notification-To`, responder MDN) | M | Ausente |
| 9 | Passos rápidos (ações encadeadas com atalho) | M | Depende de #3 |
| 10 | Limpeza (Sweep): apagar/arquivar tudo de um remetente, manter só o mais recente | M | Depende de #2 |
| 11 | Contatos: agenda, grupos, importar/exportar vCard, cartão com histórico | G | Hoje só autocompletar a partir do histórico |
| 12 | Convites de calendário (`text/calendar`): ver, aceitar/recusar/talvez | G | Ausente; exige SMTP de resposta iTIP |
| 13 | Calendário e tarefas (visão mensal, eventos, To Do) | G | Escopo de produto novo; avaliar se o Posta deve ter |
| 14 | Verificador ortográfico e sugestões ao escrever | P | Depende do WebView; confirmar |
| 15 | Relatar phishing/spam (mover para Spam já existe) | P | Falta o aviso de remetente suspeito |
| 16 | Aviso de remetente externo / endereço suspeito (Reply-To ≠ From) | P | Reply-To já é lido |
| 17 | Menções `@` e "Responder com reunião" | M | Só vale com calendário (#12) |
| 18 | Assinatura rica (imagem/HTML), várias por conta, escolha ao escrever | M | Hoje texto por conta |
| 19 | Recolher mensagem (Recall) | — | Só existe em Exchange; sem equivalente via IMAP/SMTP. Não fazer |
| 20 | Criptografia S/MIME / PGP | G | Ausente |
| 21 | Pastas de busca, arquivo morto local (PST), exportar/importar caixa | M | Opcional |
| 22 | Layout: painel de leitura embaixo/direita/desligado, lista compacta por coluna | P | Parte já em Aparência; confirmar |

P = pequeno (≤1 dia), M = médio, G = grande (spec própria).

## Próximo passo sugerido
Onda 1 (P, sem dependências): #1, #2, #4, #6, #7, #16. Onda 2: #3, #5, #8, #18. Onda 3 (spec própria): #11, #12, #13, #20.

## Levantamento no app instalado (novo Outlook v1.2026.1020.100 e clássico 16.0.20527)
Feito por UI Automation, só leitura. Ficaram de fora: submenus de contexto, Cores por Categoria, Central de Confiabilidade, Assinaturas e as janelas internas de Regra, Categoria e Programar Entrega.

### Confirmados (já na tabela)
Etapas rápidas (#9), Limpeza (#10), Regras (#3), Pastas de pesquisa (#4), Importância, Confirmação de entrega/leitura e Agendar envio (#6, #8), Relatar lixo eletrônico e phishing (#15), Bloquear e Ignorar (#7), Reunião (#17), S/MIME (#20), Arquivos de dados .pst/.ics (#21), Painel de leitura e Densidade (#22).

### Novos (23 em diante)
| # | Recurso | Versão | Esforço | Observação |
|---|---|---|---|---|
| 23 | Partes Rápidas (blocos de texto reutilizáveis) | ambos | P | O Posta tem respostas prontas; falta inserir trecho no meio do texto |
| 24 | Formatação condicional da lista (cor/negrito por regra) | ambos | M | Reaproveita o motor de regras |
| 25 | Sugestões inteligentes ao escrever e Editor (ortografia/gramática) | ambos | M | Depende de #14 |
| 26 | Encaminhamento automático de caixa | novo | P | Regra com ação "encaminhar" |
| 27 | Recuperar itens excluídos (histórico da Lixeira) | clássico | P | Depende do servidor |
| 28 | Leitura Avançada (leitor imersivo) | ambos | P | Modo foco do leitor |
| 29 | Localizar relacionados (mesma conversa/remetente) | ambos | P | Busca pré-preenchida |
| 30 | Impressão rápida e Salvar como (texto/PDF) | ambos | P | Imprimir e .eml já existem |
| 31 | Confidencialidade (Normal/Pessoal/Privado/Confidencial) | ambos | P | Cabeçalho `Sensitivity` |
| 32 | Salvar rascunho no servidor (IMAP Drafts) | ambos | M | Hoje só local; já anotado no README |
| 33 | Caixas compartilhadas e delegação ("Compartilhado comigo") | novo | G | Exige Graph/Exchange |
| 34 | Políticas de retenção, LDAP, grupos de distribuição | ambos | — | Corporativo; não fazer |
| 35 | Desenhar (tinta) na nova mensagem | novo | — | Baixo valor; não fazer |
| 36 | Horas de trabalho, clima e calendário compartilhado | novo | G | Só com calendário (#13) |
| 37 | Desfazer global (excluir, mover, arquivar) | novo | P | Hoje só desfaz envio; confirmar |
| 38 | Meu Dia (painel com agenda e tarefas) | novo | G | Só com calendário e tarefas |

### Ordem revisada
Onda 1: #1, #2, #4, #6, #7, #15, #16, #37. Onda 2: #3, #5, #8, #18, #23, #26, #29, #31, #32. Onda 3: #9, #10, #24, #25, #11, #12, #13, #20.
