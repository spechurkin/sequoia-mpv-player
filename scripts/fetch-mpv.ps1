param([Parameter(Mandatory)][string]$Target)
$ErrorActionPreference = 'Stop'
$architecture = switch ($Target)
{
    'x86_64-pc-windows-msvc' {
        'x86_64'
    }
    'aarch64-pc-windows-msvc' {
        'aarch64'
    }
    default {
        throw "Unsupported Windows mpv target: $Target"
    }
}
$runtime = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'mpv-version.json') -Raw | ConvertFrom-Json
$asset = $runtime.archives.$architecture
if (-not $asset -or $asset.sha256 -notmatch '^[a-fA-F0-9]{64}$')
{
    throw "Pinned mpv archive has no valid SHA-256 for $architecture"
}
$expectedHash = $asset.sha256.ToLowerInvariant()
$directory = Join-Path $PSScriptRoot ('../target/mpv-runtime/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $directory | Out-Null
$downloads = Join-Path $PSScriptRoot '../target/mpv-downloads'
New-Item -ItemType Directory -Force $downloads | Out-Null
$archive = Join-Path $downloads "$expectedHash.7z"
$url = $asset.url
Write-Host "Bundling pinned mpv: $( $runtime.release ) / $architecture"
if (-not (Test-Path -LiteralPath $archive) -or
        (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedHash)
{
    & curl.exe --fail --location --retry 2 --continue-at - --connect-timeout 15 --max-time 900 --output $archive $url
    if ($LASTEXITCODE)
    {
        throw 'Pinned mpv download failed'
    }
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedHash)
{
    throw 'mpv archive checksum mismatch'
}
$unpackerManifest = Join-Path $PSScriptRoot '../tools/unpack-mpv/Cargo.toml'
$unpackerTarget = Join-Path $PSScriptRoot '../target/unpack-mpv'
& cargo run --release --locked --manifest-path $unpackerManifest --target-dir $unpackerTarget -- $archive $directory | Out-Host
if ($LASTEXITCODE)
{
    throw 'mpv extraction failed'
}
@{ release = $runtime.release; version = $runtime.version; url = $url; sha256 = $expectedHash; recipes = $runtime.recipes } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $directory 'source-build.json')
if ($env:GITHUB_ENV)
{
    "SEQUOIA_MPV_DIR=$directory" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
}
Write-Output $directory
