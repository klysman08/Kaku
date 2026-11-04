<#
.SYNOPSIS
  Pluggable Authenticode signing hook, called by Tauri for every Windows binary
  (app .exe, NSIS installer, MSI) BEFORE the updater signature is computed.

.DESCRIPTION
  Select the backend with the WINDOWS_SIGN_MODE environment variable:

    none   (default) Do nothing. Builds are unsigned.
    pfx    A .pfx code-signing certificate (company/internal CA or a purchased OV cert).
             WINDOWS_CERTIFICATE           base64 of the .pfx file
             WINDOWS_CERTIFICATE_PASSWORD  its password
    azure  Azure Artifact Signing (formerly Trusted Signing), via trusted-signing-cli.
             AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET
             AZURE_SIGNING_ENDPOINT        e.g. https://eus.codesigning.azure.net/
             AZURE_SIGNING_ACCOUNT         Artifact Signing account name
             AZURE_SIGNING_PROFILE         certificate profile name
    jsign  Cloud HSM certificates (Certum SimplySign, SSL.com eSigner, Google Cloud KMS,
           DigiCert KeyLocker, ...) via jsign (https://ebourg.github.io/jsign/).
             JSIGN_JAR                     path to jsign.jar (java must be on PATH)
             JSIGN_STORETYPE, JSIGN_KEYSTORE, JSIGN_STOREPASS, JSIGN_ALIAS
             (and JSIGN_CERTFILE when the store type needs it)

  Common: WINDOWS_TIMESTAMP_URL (default http://timestamp.digicert.com).

  For SignPath Foundation (free for OSS) see docs/RELEASING.md — it signs artifacts
  after the build through its own GitHub Action.
#>
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = 'Stop'
$mode = if ($env:WINDOWS_SIGN_MODE) { $env:WINDOWS_SIGN_MODE.ToLowerInvariant() } else { 'none' }
$timestamp = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { 'http://timestamp.digicert.com' }
$description = 'KokoroKaku'

function Find-SignTool {
  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
  $found = Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $found) { throw 'signtool.exe not found (install the Windows SDK).' }
  return $found.FullName
}

function Require([string[]]$names) {
  foreach ($n in $names) {
    if (-not [Environment]::GetEnvironmentVariable($n)) { throw "WINDOWS_SIGN_MODE=$mode requires $n" }
  }
}

switch ($mode) {
  'none' {
    Write-Host "[sign] WINDOWS_SIGN_MODE=none - leaving $([IO.Path]::GetFileName($File)) unsigned"
    exit 0
  }

  'pfx' {
    Require @('WINDOWS_CERTIFICATE', 'WINDOWS_CERTIFICATE_PASSWORD')
    $pfx = Join-Path ([IO.Path]::GetTempPath()) "kokorokaku-$([Guid]::NewGuid()).pfx"
    try {
      [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
      & (Find-SignTool) sign /fd SHA256 /td SHA256 /tr $timestamp /d $description `
        /f $pfx /p $env:WINDOWS_CERTIFICATE_PASSWORD $File
      if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
    }
    finally {
      Remove-Item $pfx -Force -ErrorAction SilentlyContinue
    }
  }

  'azure' {
    Require @('AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET', 'AZURE_SIGNING_ENDPOINT', 'AZURE_SIGNING_ACCOUNT', 'AZURE_SIGNING_PROFILE')
    if (-not (Get-Command trusted-signing-cli -ErrorAction SilentlyContinue)) {
      throw 'trusted-signing-cli not found (cargo install trusted-signing-cli).'
    }
    trusted-signing-cli -e $env:AZURE_SIGNING_ENDPOINT -a $env:AZURE_SIGNING_ACCOUNT -c $env:AZURE_SIGNING_PROFILE -d $description $File
    if ($LASTEXITCODE -ne 0) { throw "trusted-signing-cli failed with exit code $LASTEXITCODE" }
  }

  'jsign' {
    Require @('JSIGN_JAR', 'JSIGN_STORETYPE', 'JSIGN_KEYSTORE', 'JSIGN_STOREPASS', 'JSIGN_ALIAS')
    $jsignArgs = @('-jar', $env:JSIGN_JAR, '--storetype', $env:JSIGN_STORETYPE, '--keystore', $env:JSIGN_KEYSTORE,
      '--storepass', $env:JSIGN_STOREPASS, '--alias', $env:JSIGN_ALIAS, '--tsaurl', $timestamp, '--tsmode', 'RFC3161',
      '--name', $description)
    if ($env:JSIGN_CERTFILE) { $jsignArgs += @('--certfile', $env:JSIGN_CERTFILE) }
    & java @jsignArgs $File
    if ($LASTEXITCODE -ne 0) { throw "jsign failed with exit code $LASTEXITCODE" }
  }

  default { throw "Unknown WINDOWS_SIGN_MODE '$mode' (use none, pfx, azure or jsign)." }
}

Write-Host "[sign] signed $([IO.Path]::GetFileName($File)) using '$mode'"
