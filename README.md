# Posta

Cliente de email pessoal, rápido e personalizado. Windows, Linux e Android (iOS depois), com Tauri 2 + React e núcleo em Rust.

## Baixar

Instaladores na página de [Releases](https://github.com/renanrmsantos14/posta/releases/latest):

- **Windows**: `Posta_x.y.z_x64-setup.exe`. Sem certificado pago, o SmartScreen avisa na primeira vez: *Mais informações* → *Executar assim mesmo*.
- **Linux**: `Posta_x.y.z_amd64.AppImage` (dar permissão de execução) ou `.deb` no Ubuntu/Debian.

Depois de instalado, o Posta se atualiza sozinho: procura versão nova ao abrir e a cada 6 h, mostra as novidades e pergunta antes de instalar. Em **Configurações** (engrenagem) dá para procurar na hora ou desligar a checagem automática.

## Como funciona

- `crates/mail-core`: sincroniza IMAP para um SQLite local (WAL + busca FTS5 sem acento). A UI só lê o banco local, por isso abre e busca na hora.
- `apps/client`: interface React. Fora do Tauri (`pnpm dev` no navegador) usa dados de exemplo de `src/mock.ts`.
- `apps/client/src-tauri`: comandos Tauri. Contas ficam em `accounts.json` na pasta de dados do app; senhas e refresh tokens OAuth no cofre do sistema (crate `keyring`).
- Chegada instantânea: no backend, uma thread por conta mantém IMAP IDLE na Caixa de entrada (renova a cada 25 min, reconecta com espera crescente de 5 s a 5 min). Ao receber aviso, sincroniza a INBOX e emite o evento `mail-changed`, que a UI escuta para recarregar. Servidor sem IDLE: a mesma thread faz polling a cada 60 s. A UI mantém um sync de segurança a cada 5 min.

## Comandos

| Comando | O que faz |
| --- | --- |
| `pnpm dev` | UI no navegador em http://localhost:36480 com dados de exemplo |
| `pnpm --filter client tauri dev` | App desktop real (precisa das dependências do Tauri no sistema) |
| `pnpm release [versão]` | Lança uma versão específica (MINOR/MAJOR) na hora: sobe a versão, cria commit e tag `vX.Y.Z` e envia |
| `pnpm --filter client tauri build` | Instalador local (exige `TAURI_SIGNING_PRIVATE_KEY` com a chave de assinatura) |
| `pnpm test` | Testes do núcleo (banco local) |
| `pnpm test:imap` | Sobe Greenmail no Docker e testa IMAP real (1.000 mensagens) e resposta por SMTP |

Linux precisa de `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev` e de um serviço de segredos (GNOME Keyring/KWallet).

## Publicar versão

Automático: cada merge no `master` (exceto mudança só em documentação) sobe o PATCH, cria a tag e publica o release. Para outra versão, `pnpm release 0.2.0` no `master` limpo. O workflow `.github/workflows/release.yml` compila Windows e Linux, assina as atualizações e publica o `latest.json` que o app consulta.

A chave de assinatura fica em `~/.tauri/posta.key` (senha em `~/.tauri/posta.key.password`) e nos secrets `TAURI_SIGNING_PRIVATE_KEY`/`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` do GitHub. **Guarde cópia dos dois arquivos**: sem eles, quem já instalou não recebe mais atualizações automáticas.

## Atalhos

`j`/`k` ou setas navegam, `/` busca, `r` responde, `c` escreve, `u` sincroniza, `Esc` limpa a busca ou volta à lista. No painel de escrita, `Ctrl`/`⌘` + `Enter` envia.

## Envio

SMTP com servidor e porta sugeridos pelo domínio (465 = TLS, outras = STARTTLS). Depois de clicar em Enviar há 4 s para desfazer; se o envio falhar, o rascunho volta intacto. Respostas levam `In-Reply-To`/`References`, a original ganha `\Answered` e uma cópia vai para Enviados (exceto Gmail, que já guarda sozinho; Outlook pode duplicar).

## Entrar com Google e Microsoft

"Adicionar conta" tem os botões **Entrar com Google** e **Entrar com Microsoft**: login no navegador do sistema (Authorization Code + PKCE, redirect em loopback), sem senha de app. O mesmo IMAP/SMTP, autenticado por XOAUTH2; o refresh token fica no cofre do sistema e o access token é renovado ao expirar. Os aplicativos OAuth precisam estar registrados e embutidos no build (`POSTA_GOOGLE_CLIENT_ID`, `POSTA_GOOGLE_CLIENT_SECRET`, `POSTA_MS_CLIENT_ID`); sem eles os botões ficam desabilitados. Passo a passo e secrets do release em [`docs/oauth.md`](docs/oauth.md). Contas antigas (senha) continuam iguais.

## Próximos passos

1. ~~Responder, escrever e enviar (SMTP).~~ Feito.
2. Arquivar e excluir; pastas além da caixa de entrada.
3. ~~IMAP IDLE para chegada instantânea.~~ Feito.
4. ~~OAuth (Google e Microsoft).~~ Feito, sem Graph/Gmail API: IMAP/SMTP com XOAUTH2. Falta validar com contas reais (veja `docs/oauth.md`).
5. Android: guardar a senha no Keystore (o `keyring` padrão não cobre Android) e sincronizar em segundo plano.
6. ~~Emails em HTML, exibidos em iframe isolado.~~ Feito: sem scripts, imagens remotas só com "Mostrar imagens", links no navegador do sistema.
