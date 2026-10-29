[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$BundleDir,
    [Parameter(Mandatory = $true)][string]$Output
)

$ErrorActionPreference = 'Stop'

function Die($message) {
    throw "error: $message"
}

if (-not (Test-Path -LiteralPath $BundleDir -PathType Container)) {
    Die "CEF bundle directory does not exist: $BundleDir"
}
$BundleDir = (Resolve-Path -LiteralPath $BundleDir).Path

foreach ($required in 'zz.exe', 'libcef.dll', 'icudtl.dat', 'resources.pak',
    'locales\en-US.pak', 'CREDITS.html', 'CEF_LICENSE.txt') {
    $path = Join-Path $BundleDir $required
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -eq 0) {
        Die "CEF bundle file is missing or empty: $required"
    }
}

$outputName = Split-Path -Leaf $Output
$outputDir = Split-Path -Parent $Output
if (-not $outputDir) {
    $outputDir = '.'
}
if (-not (Test-Path -LiteralPath $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir | Out-Null
}
$Output = Join-Path (Resolve-Path -LiteralPath $outputDir).Path $outputName
if (Test-Path -LiteralPath $Output) {
    Remove-Item -LiteralPath $Output -Force
}

$sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
if ($sevenZip) {
    & $sevenZip.Path a -tzip -mx=5 -bso0 -bsp0 -- $Output "$BundleDir\*"
    if ($LASTEXITCODE -ne 0) {
        Die "7z failed with exit code $LASTEXITCODE"
    }
} else {
    Compress-Archive -Path (Join-Path $BundleDir '*') -DestinationPath $Output -CompressionLevel Optimal
}

if (-not (Test-Path -LiteralPath $Output -PathType Leaf) -or (Get-Item -LiteralPath $Output).Length -eq 0) {
    Die "packaging did not produce an archive: $Output"
}

Write-Host "Windows archive ready: $Output"
