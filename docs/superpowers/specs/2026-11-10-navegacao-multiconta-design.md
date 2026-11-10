# Navegação entre várias contas (árvore por conta)

Data: 10/11/2026 · Decisão: variante B (árvore por conta + atalhos no topo), apelido + cor por conta.

## Objetivo

Quem usa 4 a 6 contas no Posta precisa saber, a qualquer momento, **em qual conta está**, **quantas não lidas cada conta tem** e **trocar de conta também no celular**, sem misturar as pastas de contas diferentes.

## Problemas atuais (com local no código)

1. O filtro de conta fica no rodapé da Rail (`Rail.tsx:94`), longe das pastas que ele altera; nada no cabeçalho da lista indica que o filtro está ativo.
2. Os contadores de não lidas são sempre o total de todas as contas (`App.tsx:235`; `unread_counts` em `src-tauri/src/lib.rs:534` agrega por papel).
3. "Todas as caixas" (pasta) e "Todas as contas" (filtro) são dois nomes para a mesma ideia, e a primeira ignora a segunda (`App.tsx:198`).
4. As contas não mostram não lidas; com a Rail estreita (< 1080 px), viram só a inicial.
5. No celular a Rail some (`styles.css:769`) e não existe outro caminho para filtrar por conta.

## Modelo

O filtro de conta deixa de existir como estado separado. A **pasta escolhida já carrega a conta**:

- `Todas as contas` (só com 2+ contas): visões unificadas Entrada, Com estrela e Enviados de todas as contas. Substitui "Todas as caixas" e o botão "Todas as contas".
- Uma seção por conta: cabeçalho (seta, ponto colorido, apelido, não lidas da Entrada) e, aberta, as pastas de sistema dessa conta e depois a árvore de pastas do usuário dessa conta.
- Ao abrir uma conta, as outras se recolhem (acordeão). O estado aberto fica salvo como já acontece com `openMap`.
- No topo da Rail, uma faixa de avatares (Todas + uma por conta, com contador) funciona como atalho: rola até a seção, abre e seleciona a Entrada dela. Com uma conta só, nem a faixa nem a seção "Todas" aparecem e a Rail fica como hoje.

## Identidade da conta

- **Apelido**, nova configuração `label:<id da conta>`. O padrão é a parte antes do @, ou o domínio quando essa parte se repete. Pode ser editado em Configurações › Conta, junto da cor que já existe (`accountColors.ts`).
- A cor aparece no avatar, no ponto da seção, na faixa lateral da linha da lista (em "Todas as contas") e no cabeçalho da lista.
- O cabeçalho da lista mostra sempre `Apelido › Pasta` (ou `Todas as contas › Entrada`).

## Dados

- `unread_counts` passa a devolver também as contagens por conta (`{ account, role, n }`). A consulta em `store.rs:1155` já agrupa por `account, mailbox`, então muda só a saída. O total por papel continua alimentando a bandeja.
- Contadores das pastas de usuário: sem mudança (`folderUnread`).

## Celular (≤ 720 px)

- Na barra superior, `≡` abre uma gaveta com o mesmo conteúdo da Rail: faixa de avatares, Todas as contas e as seções.
- A gaveta fecha ao tocar fora, com Voltar do Android e ao escolher uma pasta.
- O título da barra mostra `Apelido › Pasta` com o ponto colorido.

## Fora do escopo

Reordenar contas, busca de contas, regra própria de notificação por conta, mudanças no compositor (o remetente já é escolhido lá).

## Verificação

- `verificar-ui-pastas` e `verificar-ui-conta` com o mock de 2 contas, mais um mock com 5 contas:
  - com uma conta aberta, nenhuma pasta de outra conta aparece;
  - o contador de cada seção bate com as não lidas da Entrada daquela conta;
  - "Todas as contas › Entrada" soma as Entradas.
- Celular (390 px): a gaveta abre, troca de conta, fecha ao tocar fora e com Voltar, e o título reflete a conta.
- Com uma conta só, a Rail continua igual à atual.
- Fazer o `ux-audit` no modo Rápido e comparar os prints antes e depois.
