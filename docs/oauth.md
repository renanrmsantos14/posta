# Entrar com Google e Microsoft (OAuth)

O Posta usa Authorization Code + PKCE com redirect em loopback (`http://127.0.0.1:<porta aleatória>` no Google, `http://localhost:<porta>` na Microsoft) e o navegador do sistema. O token vai ao servidor por XOAUTH2 (IMAP e SMTP). O refresh token fica no cofre do sistema (serviço `posta`, usuário `<conta>:oauth-refresh`); o access token só em memória.

Os aplicativos OAuth são do Posta, não do usuário: você registra uma vez e embute os IDs no build. Sem eles, os botões ficam desabilitados.

| Variável de build | Obrigatória | O que é |
| --- | --- | --- |
| `POSTA_GOOGLE_CLIENT_ID` | para Google | ID do cliente OAuth tipo *Desktop app* |
| `POSTA_GOOGLE_CLIENT_SECRET` | quase sempre | Secret desse cliente (ver nota abaixo) |
| `POSTA_MS_CLIENT_ID` | para Microsoft | *Application (client) ID* do registro no Azure |

São lidas com `option_env!` na compilação. Nada disso é segredo de verdade em app desktop (qualquer um extrai do binário); a proteção é o PKCE. Mesmo assim, não versione: use o ambiente local e os Secrets do GitHub.

## Google

Fonte: [OAuth 2.0 para apps instalados](https://developers.google.com/identity/protocols/oauth2/native-app).

1. [Google Cloud Console](https://console.cloud.google.com/) → crie um projeto (ex.: `posta`).
2. *APIs e serviços* → *Tela de permissão OAuth* (Google Auth Platform → *Branding / Público-alvo*):
   - Tipo de usuário **Externo**, nome do app `Posta`, seu email de suporte e de contato.
   - Status de publicação: deixe em **Teste**.
   - Em *Públicos-alvo → Usuários de teste*, adicione cada conta Google que vai usar o app (até 100).
   - Em *Acesso a dados* (escopos), adicione `https://mail.google.com/`, `openid` e `email`.
3. *Credenciais* → *Criar credenciais* → *ID do cliente OAuth* → tipo de aplicativo **Aplicativo para computador (Desktop app)**. O Google aceita qualquer porta em `http://127.0.0.1`, sem registrar redirect.
4. Copie o *ID do cliente* para `POSTA_GOOGLE_CLIENT_ID` e o *Secret* para `POSTA_GOOGLE_CLIENT_SECRET`. A documentação marca o secret como opcional para apps instalados, mas clientes Desktop costumam recebê-lo e o endpoint de token pode recusar a troca sem ele (`client_secret is missing`). Se a troca funcionar sem, deixe vazio.

Limitações do modo Teste (regras do Google, confira no console):
- `https://mail.google.com/` é escopo **restrito**. Sem verificação, só contas listadas como usuários de teste entram, e o login mostra "app não verificado" (*Avançado → Acessar Posta*).
- Em Teste, o refresh token expira em **7 dias**: depois disso o Posta pede para entrar de novo naquela conta.
- Para sair do Teste é preciso verificação do app e, para escopos restritos, avaliação de segurança paga. Para uso pessoal, o modo Teste basta.

## Microsoft

Fontes: [IMAP/SMTP com OAuth no Exchange Online](https://learn.microsoft.com/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth) e [fluxo de código de autorização](https://learn.microsoft.com/entra/identity-platform/v2-oauth2-auth-code-flow).

1. [Portal do Azure](https://portal.azure.com/) → *Microsoft Entra ID* → *Registros de aplicativo* → *Novo registro*.
2. Nome `Posta`. Tipos de conta: **Contas em qualquer diretório organizacional e contas pessoais da Microsoft** (para funcionar com outlook.com/hotmail e Microsoft 365).
3. URI de redirecionamento: plataforma **Aplicativos móveis e de área de trabalho**, valor `http://localhost`. A Microsoft ignora a porta no loopback, por isso o Posta usa `http://localhost:<porta>`; o literal `127.0.0.1` não é aceito aqui.
4. *Autenticação* → *Configurações avançadas* → **Permitir fluxos de cliente público = Sim**. Não crie client secret.
5. *Permissões de API* → *Adicionar permissão* → *APIs que minha organização usa* → **Office 365 Exchange Online** → *Permissões delegadas*: `IMAP.AccessAsUser.All` e `SMTP.Send`. Também `offline_access`, `openid`, `email`, `profile` (Microsoft Graph, delegadas). Para uso próprio o consentimento é pedido no primeiro login; um tenant corporativo pode exigir consentimento do administrador.
6. Copie o *ID do aplicativo (cliente)* para `POSTA_MS_CLIENT_ID`.

Observações:
- Em contas Microsoft 365, o administrador precisa deixar o **SMTP AUTH** habilitado para a caixa (Exchange admin → usuário → *Gerenciar aplicativos de email*) e o IMAP ativo.
- A Microsoft gira o refresh token a cada uso; o Posta regrava o novo no cofre.

## Build local

```sh
export POSTA_GOOGLE_CLIENT_ID=...apps.googleusercontent.com
export POSTA_GOOGLE_CLIENT_SECRET=...
export POSTA_MS_CLIENT_ID=...
pnpm --filter client tauri dev
```

## Release no GitHub Actions

Repositório → *Settings → Secrets and variables → Actions → New repository secret*: crie `POSTA_GOOGLE_CLIENT_ID`, `POSTA_GOOGLE_CLIENT_SECRET` e `POSTA_MS_CLIENT_ID`. O `release.yml` já os passa ao passo de compilação. Se faltar algum, o release sai normalmente com o botão correspondente desabilitado.

## Como testar

Com o app compilado com os IDs: *Adicionar conta* → *Entrar com Google/Microsoft* → autorize no navegador → a aba mostra "Conta conectada" e o Posta sincroniza. Falhas comuns: `redirect_uri_mismatch` (tipo de cliente errado no Google, ou redirect não é `http://localhost` na Microsoft), `access_denied` (conta fora dos usuários de teste), `invalid_grant` ao renovar (refresh token expirado ou revogado: entre de novo).
