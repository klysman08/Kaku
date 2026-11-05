# Release, versionamento e assinatura

## TL;DR

```bash
pnpm release patch      # 0.1.0 → 0.1.1   (ou minor / major / 1.2.3)
```

O script (`scripts/release.mjs`):

1. exige working tree limpa e tag inexistente;
2. atualiza a versão em `package.json` (fonte única — `tauri.conf.json` lê dela) e em `src-tauri/Cargo.toml`/`Cargo.lock`;
3. cria o commit `chore(release): vX.Y.Z` e a tag anotada `vX.Y.Z`;
4. faz `git push --follow-tags` (use `--no-push` para revisar antes, `--dry-run` para só ver a próxima versão).

O push da tag dispara `.github/workflows/release.yml`, que:

- confere se a tag bate com `package.json`;
- compila Windows (NSIS `.exe` + `.msi`) e Linux (`.deb`, `.rpm`, `.AppImage`);
- assina os binários Windows se houver assinatura configurada (abaixo);
- assina os pacotes do **updater** e gera `latest.json`;
- cria um **release em rascunho** com notas automáticas, `SHA256SUMS.txt` e attestations de proveniência.

Revise o rascunho e clique em **Publish release**. Só então as instalações existentes enxergam a atualização (o endpoint é `releases/latest/download/latest.json`).

Versões com hífen (`1.0.0-beta.1`) saem como *pre-release* e não são entregues pelo updater.

## Secrets do repositório (Settings › Secrets and variables › Actions)

### Obrigatórios — assinatura do updater

| Secret | Valor |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | conteúdo de `~/.tauri/kaku.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | senha da chave (vazio se gerada sem senha) |

A chave pública correspondente já está em `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). **Guarde a chave privada num cofre** (ex.: gerenciador de senhas): sem ela não é possível publicar atualizações para quem já instalou. Para gerar uma nova (invalida o updater das versões anteriores):

```bash
pnpm tauri signer generate -w ~/.tauri/kaku.key
```

### Opcionais — assinatura de código Windows (Authenticode)

Escolha o backend na **variável** `WINDOWS_SIGN_MODE` (aba *Variables*). Sem ela, os builds saem sem assinatura (`none`). O hook é `scripts/sign-windows.ps1`, chamado pelo Tauri para cada `.exe`/`.msi` **antes** da assinatura do updater.

| Modo | Quando usar | Configuração |
|---|---|---|
| `pfx` | Certificado da empresa (CA interna) ou OV comprado em `.pfx` | secrets `WINDOWS_CERTIFICATE` (base64 do .pfx: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`) e `WINDOWS_CERTIFICATE_PASSWORD` |
| `azure` | Azure Artifact Signing (Trusted Signing) — exige organização elegível; pessoa física só EUA/Canadá | secrets `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`; variables `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE` |
| `jsign` | Certificado OV em HSM na nuvem (Certum SimplySign, SSL.com eSigner, DigiCert KeyLocker, Google KMS…) | `JSIGN_*` (ver cabeçalho do script) e um passo que baixe `jsign.jar` |

Opcional: variável `WINDOWS_TIMESTAMP_URL` (padrão `http://timestamp.digicert.com`).

#### Qual escolher?

- **Instalar só no PC da empresa:** o caminho mais rápido costuma ser pedir ao TI um certificado de assinatura de código da CA interna (modo `pfx`) — máquinas do domínio já confiam nela. Alternativa: o TI adiciona o hash/assinatura do instalador a uma allowlist (AppLocker/WDAC/Intune).
- **Distribuir publicamente:**
  - [SignPath Foundation](https://signpath.org/) — grátis para projetos OSS com licença OSI, repositório público e build no CI. Após aprovação, adicione o passo `signpath/github-action-submit-signing-request` no workflow (assina os artefatos depois do build; nesse caso, regenere a assinatura do updater sobre os arquivos assinados ou desative `createUpdaterArtifacts` no Windows).
  - Certificado **OV** (~US$ 150–300/ano; a Certum tem oferta para open source) via `jsign` ou `pfx`.
- O SmartScreen ganha reputação aos poucos mesmo com certificado OV; o aviso some depois de algumas centenas de instalações.

Teste local da assinatura:

```powershell
$env:WINDOWS_SIGN_MODE = "pfx"; $env:WINDOWS_CERTIFICATE = "<base64>"; $env:WINDOWS_CERTIFICATE_PASSWORD = "<senha>"
pnpm tauri build
Get-AuthenticodeSignature .\src-tauri\target\release\bundle\nsis\*.exe
```

### macOS (futuro)

Quando houver conta Apple Developer (US$ 99/ano), adicione `macos-latest` à matriz e os secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` (o `tauri-action` faz assinatura e notarização).

## Rebuild de uma tag

*Actions › Release › Run workflow* com `tag = vX.Y.Z` reconstrói os artefatos de uma tag existente.

## Checklist antes de publicar

- [ ] CI verde em `main`
- [ ] Testar o instalador do rascunho numa máquina limpa (atalhos, paleta, substituição, teste de conexão)
- [ ] `Get-AuthenticodeSignature` mostra `Valid` (se assinatura configurada)
- [ ] Notas do release revisadas
