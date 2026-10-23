param([string]$MpvDirectory, [switch]$RefreshWebTools, [string]$Repository = $env:GITHUB_REPOSITORY)
$ErrorActionPreference = 'Stop'
if (-not $MpvDirectory)
{
    $MpvDirectory = & (Join-Path $PSScriptRoot 'fetch-mpv.ps1') -Target x86_64-pc-windows-msvc
}
if (-not (Test-Path -LiteralPath (Join-Path $MpvDirectory 'mpv.exe') -PathType Leaf))
{
    throw 'mpv.exe is required to build a self-contained package. Supply -MpvDirectory.'
}
Push-Location (Join-Path $PSScriptRoot '..')
try
{
    cargo build --release --locked
    if ($LASTEXITCODE)
    {
        throw 'Rust release build failed'
    }
    $packageName = 'Sequoia-windows-x64'
    $webArgs = @()
    if ($RefreshWebTools)
    {
        $webArgs += '--refresh-web-tools'
    }
    if ($Repository)
    {
        $webArgs += @('--repository', $Repository)
    }
    python scripts/package-ci.py --target x86_64-pc-windows-msvc --label local --minimum 10 `
        --binary target/release/sequoia.exe --mpv $MpvDirectory --output-name $packageName @webArgs
    if ($LASTEXITCODE)
    {
        throw 'Portable packaging failed'
    }
}
finally
{
    Pop-Location
}
