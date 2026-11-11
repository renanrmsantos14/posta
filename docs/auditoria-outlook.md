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

## Levantamento no app instalado (parcial)
Feito no novo Outlook v1.2026.1020.100 (Betinhos-Renan), só a aba Página Inicial; Exibir, Ajuda, Arquivo, Configurações, Nova mensagem e menus de contexto não foram abertos, e o Outlook clássico não abriu (encerra em segundos sem janela).
Itens vistos na Página Inicial e o que muda na tabela acima:
- Já listados: Excluir, Arquivar, Responder, Responder a todos, Encaminhar, Mover, Lido/Não lido, Categorizar, Bandeira, Fixar, Adiar, Imprimir, Destaques/Outros, Pastas de Pesquisa (#4, #21).
- **Novos**: Etapas rápidas (#9 confirmado), Relatar lixo eletrônico (#15), Desfazer global de ações (o Posta só desfaz o envio), responder com Reunião (#17), Descobrir grupos (fora de escopo em IMAP), pastas Observações, Arquivo Morto, Histórico de Conversa e RSS Feeds (RSS fora de escopo).
