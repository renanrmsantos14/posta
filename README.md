# Posta

Cliente de email pessoal, rápido e personalizado. Windows, Linux e Android (iOS depois), com Tauri 2 + React e núcleo em Rust.

## Baixar

Instaladores na página de [Releases](https://github.com/renanrmsantos14/posta/releases/latest):

- **Windows**: `Posta_x.y.z_x64-setup.exe`. Sem certificado pago, o SmartScreen avisa na primeira vez: *Mais informações* → *Executar assim mesmo*.
- **Linux**: `Posta_x.y.z_amd64.AppImage` (dar permissão de execução) ou `.deb` no Ubuntu/Debian.

- **Android**: `Posta_x.y.z_android.apk` (nos Releases, ou no artefato `posta-android-apk` da execução do workflow *Android*). Baixe no celular, abra o arquivo e permita "instalar apps desconhecidos" para o navegador/gerenciador quando o Android pedir. O APK do release é assinado com uma chave estável (secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`; o job `android` do workflow Release falha sem eles; gere a chave com `keytool -genkeypair -v -keystore posta-release.keystore -alias posta -keyalg RSA -keysize 2048 -validity 36500` e o secret com `base64 -w0 posta-release.keystore`), e o app avisa "Nova versão vX disponível" ao abrir, ao voltar para o app e a cada 2 min, com o botão Atualizar que baixa o APK (o Android pede para instalar por cima). Builds de PR (workflow *Android*) usam chave de debug e não atualizam por cima do release: desinstale antes. A senha da conta fica no Android Keystore (SharedPreferences cifrado). Não há sincronização em segundo plano: o app sincroniza ao abrir, ao voltar para o primeiro plano e a cada 60 s enquanto aberto (WorkManager fica para depois).

Depois de instalado no desktop, o Posta se atualiza sozinho: procura versão nova ao abrir, ao voltar para o app e a cada 2 min, mostra as novidades e pergunta antes de instalar. Em **Configurações** (engrenagem) dá para procurar na hora ou desligar a checagem automática.

## Como funciona

- `crates/mail-core`: sincroniza IMAP para um SQLite local (WAL + busca FTS5 sem acento). A UI só lê o banco local, por isso abre e busca na hora.
- `apps/client`: interface React. Fora do Tauri (`pnpm dev` no navegador) usa dados de exemplo de `src/mock.ts`.
- `apps/client/src-tauri`: comandos Tauri. Contas ficam em `accounts.json` na pasta de dados do app; senhas e refresh tokens OAuth no cofre do sistema (crate `keyring`; no Android, Keystore via `android-native-keyring-store`, em `src/secret.rs`).
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

`j`/`k` ou setas navegam, `/` busca, `r` responde, `a` responde a todos (remetente em Para, os demais em Cc, sem você), `f` encaminha (leva os anexos da original), `c` escreve, `e` arquiva, `#` exclui, `s` marca com estrela, `u` marca como não lida, `Esc` limpa a busca, a seleção ou volta à lista. Com mensagens marcadas (caixa de seleção; Shift marca um intervalo), `e`, `#`, `s` e `u` valem para todas. Atalhos por teclado não têm animação. "Com estrela" é uma visão local (exclui Lixeira e Spam); lida e estrela sincronizam com o servidor (`\Seen`, `\Flagged`). No painel de escrita, `Ctrl`/`⌘` + `Enter` envia.

## Rascunhos, assinatura e contas

O painel de escrita salva sozinho (1,5 s após parar de digitar; Cc e Bcc entram no rascunho, anexos não) no banco local; a pasta **Rascunhos** da barra lateral lista e reabre. Fechar com Esc ou X guarda o rascunho; a lixeira descarta (pergunta se há texto); enviar remove o rascunho quando o envio dá certo. Os rascunhos ficam só neste aparelho (não são sincronizados com a pasta Drafts do IMAP).

Em **Configurações**: assinatura por conta (entra em mensagens novas e respostas), notificações de email novo e **Remover conta** ("Remover conta?" apaga senha/refresh do cofre e mensagens, pastas e rascunhos locais; o servidor não é tocado). Com várias contas, a Caixa de entrada junta todas e a barra lateral filtra por conta ("Todas as contas" volta).

Notificações: quando a IDLE ou o sync traz mensagens novas e a janela não está em foco, o sistema mostra remetente e assunto (até 3; acima disso, um resumo). O título da janela leva a contagem de não lidas. No Android 13+ o app pede a permissão ao abrir. Clicar na notificação traz o app para a frente (mesmo escondido na bandeja ou minimizado), vai à conta e à pasta e abre aquele email no leitor; se a notificação resumia vários, abre a Caixa de entrada da conta, e se o email já saiu da pasta, abre a pasta com um aviso. No Windows o clique só é entregue com o aviso ainda na tela (não depois de ir para a Central de ações).

## Envio

SMTP com servidor e porta sugeridos pelo domínio (465 = TLS, outras = STARTTLS). Depois de clicar em Enviar há 4 s para desfazer; se o envio falhar, o rascunho volta intacto. Cc e Bcc (botão "Cc/Bcc") aceitam vários endereços separados por vírgula ou `;`. O clipe do painel anexa arquivos (até 25 MB no total; vai como `multipart/mixed`). Ao ler, os anexos aparecem como chips: clicar baixa do servidor, salva em Downloads e abre com o programa padrão; imagens inline (`cid:`) aparecem no corpo. Respostas levam `In-Reply-To`/`References`, a original ganha `\Answered` e uma cópia vai para Enviados (exceto Gmail, que já guarda sozinho; Outlook pode duplicar).

## Entrar com Google e Microsoft

"Adicionar conta" tem os botões **Entrar com Google** e **Entrar com Microsoft**: login no navegador do sistema (Authorization Code + PKCE, redirect em loopback), sem senha de app. O mesmo IMAP/SMTP, autenticado por XOAUTH2; o refresh token fica no cofre do sistema e o access token é renovado ao expirar. Os Client IDs não ficam no código: colados em Configurações › Contas › Avançado ou embutidos no build (`POSTA_GOOGLE_CLIENT_ID`, `POSTA_GOOGLE_CLIENT_SECRET`, `POSTA_MS_CLIENT_ID`); sem eles os botões ficam desabilitados. Se o provedor encerrar o acesso (ex.: Gmail em modo teste, 7 dias), o aviso traz o botão "Entrar de novo". Passo a passo e secrets do release em [`docs/oauth-setup.md`](docs/oauth-setup.md). Contas antigas (senha) continuam iguais.

## Próximos passos

1. ~~Responder, escrever e enviar (SMTP).~~ Feito.
2. Arquivar e excluir; pastas além da caixa de entrada.
3. ~~IMAP IDLE para chegada instantânea.~~ Feito.
4. ~~OAuth (Google e Microsoft).~~ Feito, sem Graph/Gmail API: IMAP/SMTP com XOAUTH2. Falta validar com contas reais (veja `docs/oauth-setup.md`).
5. ~~Android: APK, senha no Keystore.~~ Feito. Falta sincronizar em segundo plano (WorkManager, 15 min) e login OAuth no Android; as notificações só chegam com o app aberto (sem serviço em segundo plano).
6. ~~Emails em HTML, exibidos em iframe isolado.~~ Feito: sem scripts, imagens remotas só com "Mostrar imagens", links no navegador do sistema.
