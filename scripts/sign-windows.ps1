[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)][string[]]$Paths
)

$ErrorActionPreference = 'Stop'

function Die($message) {
    throw "error: $message"
}

$pfxBase64 = $env:WINDOWS_CERTIFICATE_PFX_BASE64
$password = $env:WINDOWS_CERTIFICATE_PASSWORD
$timestampUrl = $env:WINDOWS_TIMESTAMP_URL
if (-not $timestampUrl) {
    $timestampUrl = 'http://timestamp.digicert.com'
}

if (-not $pfxBase64) {
    Die 'WINDOWS_CERTIFICATE_PFX_BASE64 is unset; nothing to sign with'
}
if (-not $password) {
    Die 'WINDOWS_CERTIFICATE_PASSWORD is unset'
}

foreach ($path in $Paths) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Die "file to sign does not exist: $path"
    }
}

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($signtool) {
    $signtoolPath = $signtool.Path
} else {
    $signtoolPath = Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\bin' `
        -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like '*\x64\*' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $signtoolPath) {
    Die 'signtool.exe not found; install the Windows SDK signing tools'
}

$certificate = Join-Path ([System.IO.Path]::GetTempPath()) "zz-signing-$([guid]::NewGuid()).pfx"
try {
    [System.IO.File]::WriteAllBytes($certificate, [System.Convert]::FromBase64String($pfxBase64))

    & $signtoolPath sign /fd sha256 /td sha256 /tr $timestampUrl `
        /f $certificate /p $password $Paths
    if ($LASTEXITCODE -ne 0) {
        Die "signtool sign failed with exit code $LASTEXITCODE"
    }

    & $signtoolPath verify /pa /v $Paths
    if ($LASTEXITCODE -ne 0) {
        Die "signtool verify failed with exit code $LASTEXITCODE"
    }
} finally {
    Remove-Item -LiteralPath $certificate -Force -ErrorAction SilentlyContinue
}

Write-Host "signed: $($Paths -join ', ')"
