# Entrar com Google e Microsoft: configuração

O Posta entra em contas Gmail, Google Workspace, Outlook.com/Hotmail e Microsoft 365 pelo navegador, sem senha de app. Usa OAuth 2.0 (Authorization Code + PKCE, redirect em loopback) e autentica o IMAP/SMTP já existente por XOAUTH2. Não usa a Gmail API nem o Microsoft Graph.

Os aplicativos OAuth são seus: você cria um no Google e outro na Microsoft (uns 10 minutos cada, gratuito) e cola os IDs no Posta. Sem os IDs, os botões "Entrar com Google/Microsoft" ficam desabilitados.

## Onde colar os IDs

Duas formas; vale a digitada, depois a embutida no build.

1. **No app** (mais simples): *Configurações › Contas › Avançado: Client IDs…*. Cole o ID do Google (e o segredo, se o Google der um), o ID da Microsoft e clique em *Salvar IDs*. Os botões habilitam na hora. Ficam em `oauth-clients.json` na pasta de dados do app, não no código nem no Git.
2. **Na compilação**, para distribuir o app já configurado:

| Variável de build | Para | O que é |
| --- | --- | --- |
| `POSTA_GOOGLE_CLIENT_ID` | Google | ID do cliente OAuth tipo *App para computador* |
| `POSTA_GOOGLE_CLIENT_SECRET` | Google | Segredo desse cliente (ver nota no passo 4) |
| `POSTA_MS_CLIENT_ID` | Microsoft | *ID do aplicativo (cliente)* do registro no Entra |

```sh
export POSTA_GOOGLE_CLIENT_ID=...apps.googleusercontent.com
export POSTA_GOOGLE_CLIENT_SECRET=...
export POSTA_MS_CLIENT_ID=...
pnpm --filter client tauri dev
```

No GitHub: *Settings › Secrets and variables › Actions › New repository secret* com os mesmos três nomes; o `release.yml` os passa à compilação. Faltando algum, o release sai normalmente com o botão correspondente desabilitado.

Em app de desktop esses valores não são segredos de verdade (qualquer um extrai do binário); a proteção é o PKCE. Mesmo assim, não os versione.

## Google (Gmail pessoal e Workspace)

Fonte: [OAuth 2.0 para apps instalados](https://developers.google.com/identity/protocols/oauth2/native-app).

1. Entre em [console.cloud.google.com](https://console.cloud.google.com/) e crie um projeto (ex.: `posta`). Não é preciso ativar nenhuma API: IMAP e SMTP não passam por API do Google, o escopo `https://mail.google.com/` vale para eles.
2. Menu *APIs e serviços › Tela de permissão OAuth* (ou *Google Auth Platform*) e *Começar*:
   - Nome do app `Posta`, email de suporte e de contato.
   - **Público**: **Externo** para Gmail pessoal (e para Workspace de outras organizações); **Interno** só se todas as contas forem do seu próprio Workspace (então não há modo de teste, nem aviso de "app não verificado", nem expiração de 7 dias).
   - Mantenha o status de publicação em **Teste**.
   - Em *Público-alvo › Usuários de teste* (apenas Externo), adicione cada Gmail que vai usar o Posta (até 100).
   - Em *Acesso a dados › Adicionar ou remover escopos*, inclua `https://mail.google.com/`, `openid` e `email`.
3. *Credenciais › Criar credenciais › ID do cliente OAuth › Tipo de aplicativo: **App para computador*** (Desktop app). O Google aceita qualquer porta em `http://127.0.0.1` sem registrar redirect.
4. Copie o *ID do cliente* e o *Segredo do cliente*. A documentação trata o segredo como opcional em app instalado, mas clientes Desktop costumam recebê-lo e o endpoint de token pode recusar a troca sem ele (`client_secret is missing`). Cole o que existir; se a troca funcionar sem o segredo, deixe vazio.

Limites do modo Teste (regras do Google):
- `https://mail.google.com/` é escopo **restrito**. Sem verificação, só entram as contas listadas como usuários de teste, e o login mostra "app não verificado" (*Avançado › Acessar Posta*).
- O refresh token expira em **7 dias**. O Posta avisa ("O Google encerrou o acesso…") com o botão **Entrar de novo**.
- Sair do Teste exige verificação do app e, para escopos restritos, avaliação de segurança paga. Para uso pessoal, o modo Teste basta.
- Workspace: o administrador precisa permitir apps de terceiros / acesso por IMAP nas configurações do domínio (*Apps › Google Workspace › Gmail › Acesso de usuário final*) e, se bloquear apps não confiáveis, marcar o ID do cliente como confiável (*Segurança › Controles de API*).

## Microsoft (Outlook.com, Hotmail e Microsoft 365)

Fontes: [IMAP/SMTP com OAuth no Exchange Online](https://learn.microsoft.com/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth) e [fluxo de código de autorização](https://learn.microsoft.com/entra/identity-platform/v2-oauth2-auth-code-flow).

1. [entra.microsoft.com](https://entra.microsoft.com/) (ou portal.azure.com) › *Identidade › Aplicativos › Registros de aplicativo › Novo registro*.
2. Nome `Posta`. Tipos de conta: **Contas em qualquer diretório organizacional (Qualquer diretório do Microsoft Entra – Multilocatário) e contas pessoais da Microsoft**. O Posta usa o endpoint `common`, que atende as duas.
3. URI de redirecionamento: plataforma **Aplicativos móveis e de desktop**, valor `http://localhost`. A Microsoft ignora a porta no loopback, por isso o Posta usa `http://localhost:<porta>`; `127.0.0.1` não é aceito aqui.
4. Depois de criar: *Autenticação › Configurações avançadas › Permitir fluxos de cliente público* = **Sim**. Não crie client secret.
5. *Permissões de API › Adicionar uma permissão › APIs que minha organização usa › **Office 365 Exchange Online** › Permissões delegadas*: `IMAP.AccessAsUser.All` e `SMTP.Send`. Em *Microsoft Graph › Delegadas*: `offline_access`, `openid`, `email`, `profile`. Não adicione mais nada.
6. Copie o *ID do aplicativo (cliente)* da página *Visão geral* e cole em *Configurações › Contas › Avançado › Microsoft*.

Observações para Microsoft 365 (conta de trabalho ou escola):
- **Consentimento do administrador**: muitos tenants bloqueiam o consentimento do usuário. Nesse caso o login falha com "precisa de aprovação do administrador". O admin concede em *Entra › Aplicativos empresariais › Posta › Permissões › Conceder consentimento do administrador*, ou pela URL `https://login.microsoftonline.com/<tenant>/adminconsent?client_id=<ID>`.
- **SMTP AUTH precisa estar habilitado na caixa** (Exchange admin › Usuários › o usuário › Email › *Gerenciar aplicativos de email* › *SMTP autenticado*). Sem isso a leitura funciona e o envio falha; o Posta avisa "O SMTP AUTH está desativado nesta caixa".
- O IMAP também precisa estar ativo para a caixa.
- Contas Outlook.com/Hotmail pessoais não têm esses bloqueios.
- A Microsoft gira o refresh token a cada uso; o Posta regrava o novo no cofre do sistema.

## Como o Posta se comporta

- O **refresh token** fica no cofre do sistema (serviço `posta`, entrada `<conta>:oauth-refresh`), onde as senhas já ficam. O **access token** só existe em memória e é renovado quando faltam menos de 60 s para vencer.
- Se o servidor recusar o token mesmo assim (revogado, relógio, vencimento antecipado), o Posta renova e repete a operação **uma vez**. Falha seguida de nova falha não entra em laço.
- Quando o provedor encerra o acesso (token de 7 dias do Google em modo Teste, senha trocada, consentimento pendente do admin), o aviso explica o motivo e traz o botão **Entrar de novo**; também há um em *Configurações › Contas*. O login refeito só vale para a mesma conta.
- Tokens nunca vão para log, arquivo de contas ou mensagens de erro.

## Android

Não está disponível ainda; o botão mostra uma mensagem pedindo a senha de app. O loopback `127.0.0.1` não é um fluxo suportado para apps Android: seria preciso criar clientes próprios (Google: tipo *Android* com pacote e SHA-1; Microsoft: plataforma *Android* com redirect `msauth://…`) e registrar um deep link/esquema customizado no app com o plugin `deep-link` do Tauri 2. O desktop já está pronto para isso (o fluxo de login é isolado em `apps/client/src-tauri/src/oauth_app.rs`).

## Como testar com os IDs reais

*Adicionar conta › Entrar com Google/Microsoft* › autorize no navegador › a aba mostra "Conta conectada" e o Posta sincroniza. Para forçar a renovação, apague o cache reiniciando o app (o access token só vive em memória).

Falhas comuns:
- `redirect_uri_mismatch` (Google): o cliente não é *App para computador*. Microsoft: o redirect não é `http://localhost` na plataforma *Aplicativos móveis e de desktop*.
- `access_denied` / "app não verificado": a conta não está nos usuários de teste.
- `AADSTS65001` / `AADSTS90094`: falta consentimento do administrador.
- `invalid_client` ao trocar o código: Client ID errado, ou o Google exige o segredo do cliente.
- `AUTHENTICATE failed` / `535 5.7.139`: IMAP ou SMTP AUTH desativado na caixa do Microsoft 365.
