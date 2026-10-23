#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Repository,
    [switch]$Check,
    [switch]$InstallTask,
    [switch]$RemoveTask,
    [switch]$Quiet
)
$ErrorActionPreference = 'Stop'

function Read-Json($Path)
{
    Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Get-Version($Value)
{
    if ($Value -notmatch '^v?(\d+)\.(\d+)\.(\d+)$')
    {
        throw "Unsupported stable version: $Value"
    }
    [version]($Value -replace '^v', '')
}

function Get-PackagePath($Root, $Relative)
{
    if ([string]::IsNullOrWhiteSpace($Relative) -or $Relative.Contains('\') -or
            [IO.Path]::IsPathRooted($Relative) -or $Relative.Contains(':') -or
            @($Relative.Split('/') | Where-Object { $_ -in @('', '.', '..') -or $_ -match '[. ]$' }).Count)
    {
        throw "Unsafe package path: $Relative"
    }
    $full = [IO.Path]::GetFullPath((Join-Path $Root $Relative))
    if (-not $full.StartsWith($Root.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase))
    {
        throw "Path outside package: $Relative"
    }
    $full
}

function Get-ManagedFiles($Info, $Root)
{
    $files = @($Info.managed_files)
    if (-not $files.Count -or 'docs/build-info.json' -notin $files -or 'Sequoia.exe' -notin $files -or
            'installers/updater.ps1' -notin $files -or 'installers/update.cmd' -notin $files)
    {
        throw 'Incomplete package manifest.'
    }
    $seen = @{ }
    foreach ($file in $files)
    {
        $null = Get-PackagePath $Root $file
        if ( $seen.ContainsKey($file))
        {
            throw "Duplicate package path: $file"
        }
        $seen[$file] = $true
        if ($file -in @('update-settings.json', '.update.lock', 'update-error.log',
        'installers/update-settings.json', 'installers/.update.lock', 'installers/update-error.log'))
        {
            throw "Reserved package file: $file"
        }
    }
    $files
}

function Assert-NoLinks($Root)
{
    $item = Get-Item -LiteralPath $Root -Force
    while ($null -ne $item)
    {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
        {
            throw 'Linked installation paths are unsupported.'
        }
        $item = $item.Parent
    }
    foreach ($item in Get-ChildItem -LiteralPath $Root -Recurse -Force)
    {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
        {
            throw "Linked package path: $( $item.FullName )"
        }
    }
}

function Test-PlayerRunning($Root)
{
    $prefix = $Root.TrimEnd('\') + '\'
    foreach ($process in Get-Process -Name Sequoia,mpv,yt-dlp,deno -ErrorAction SilentlyContinue)
    {
        if ($process.Path -and $process.Path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase))
        {
            return $true
        }
    }
    return $false
}

function Sync-InstalledVersion($Root, $Version)
{
    $path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Sequoia'
    if (Test-Path -LiteralPath $path)
    {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($path.Substring(6), $true)
        try
        {
            if ($key.GetValue('InstallLocation') -ieq $Root)
            {
                $key.SetValue('DisplayVersion', [string]$Version, [Microsoft.Win32.RegistryValueKind]::String)
            }
        }
        finally
        {
            $key.Dispose()
        }
    }
}

function Expand-Package($Archive, $Destination)
{
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try
    {
        $roots = @{ }
        $names = @{ }
        foreach ($entry in $zip.Entries)
        {
            $name = $entry.FullName.TrimEnd('/')
            $null = Get-PackagePath $Destination $name
            if ( $names.ContainsKey($name))
            {
                throw "Duplicate ZIP entry: $name"
            }
            $names[$name] = $true
            $roots[$name.Split('/')[0]] = $true
        }
        if ($roots.Count -ne 1)
        {
            throw 'Expected one package directory in ZIP.'
        }
        [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $Destination)
        $root = Join-Path $Destination @($roots.Keys)[0]
        Assert-NoLinks $root
        return $root
    }
    finally
    {
        $zip.Dispose()
    }
}

function Install-Package($Source, $Destination, $OldFiles, $NewFiles, $Backup)
{
    Assert-NoLinks $Destination
    $all = @(@($OldFiles) + @($NewFiles) | Sort-Object -Unique)
    # Reject locks and user-file collisions before touching the installation.
    foreach ($file in $all)
    {
        $path = Get-PackagePath $Destination $file
        if (Test-Path -LiteralPath $path)
        {
            if ($file -notin $OldFiles)
            {
                throw "Update would replace a user file: $file"
            }
            $handle = [IO.File]::Open($path, 'Open', 'ReadWrite', 'None')
            $handle.Dispose()
            $copy = Get-PackagePath $Backup $file
            $null = New-Item -ItemType Directory -Path (Split-Path $copy) -Force
            Copy-Item -LiteralPath $path -Destination $copy
        }
    }
    $touched = @()
    try
    {
        foreach ($file in $all)
        {
            $path = Get-PackagePath $Destination $file
            $touched += $file
            if ($file -in $NewFiles)
            {
                $null = New-Item -ItemType Directory -Path (Split-Path $path) -Force
                Copy-Item -LiteralPath (Get-PackagePath $Source $file) -Destination $path -Force
            }
            elseif (Test-Path -LiteralPath $path)
            {
                Remove-Item -LiteralPath $path -Force
            }
        }
    }
    catch
    {
        $failure = $_
        foreach ($file in $touched)
        {
            $path = Get-PackagePath $Destination $file
            $copy = Get-PackagePath $Backup $file
            if (Test-Path -LiteralPath $copy)
            {
                Copy-Item -LiteralPath $copy -Destination $path -Force
            }
            elseif (Test-Path -LiteralPath $path)
            {
                Remove-Item -LiteralPath $path -Force
            }
        }
        throw $failure
    }
}

function Invoke-Updater
{
    $root = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
    Assert-NoLinks $root
    $info = Read-Json (Join-Path $root 'docs/build-info.json')
    $oldFiles = @(Get-ManagedFiles $info $root)
    $configPath = Join-Path $root 'installers/update-settings.json'
    $config = if (Test-Path -LiteralPath $configPath)
    {
        Read-Json $configPath
    }
    else
    {
        $null
    }
    if (-not $Repository)
    {
        $Repository = if ($config)
        {
            $config.repository
        }
        else
        {
            $info.update_repository
        }
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try
    {
        $id = ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($root.ToLowerInvariant())))).Replace('-', '').Substring(0, 16)
    }
    finally
    {
        $sha.Dispose()
    }
    $taskName = "Sequoia Update $id"
    if ($RemoveTask)
    {
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
        Write-Output 'Automatic updates disabled.'
        return
    }
    if ($Repository -notmatch '^[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+$')
    {
        throw 'Specify the GitHub repository: update.cmd -Repository OWNER/sequoia-mpv-player'
    }
    if ($InstallTask)
    {
        $script = Join-Path $root 'installers/updater.ps1'
        $action = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument "-NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File `"$script`" -Quiet"
        $trigger = New-ScheduledTaskTrigger -Daily -At '12:00'
        $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 30)
        $principal = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited
        $null = Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force
        @{ repository = $Repository } | ConvertTo-Json | Set-Content -LiteralPath $configPath -Encoding UTF8
        Write-Output 'Automatic updates enabled: daily at 12:00, while logged in. Close Sequoia using Exit in the tray menu.'
        return
    }
    if (Test-PlayerRunning $root)
    {
        Write-Output 'Sequoia or its playback tools are running; update deferred. Use Exit in the tray menu.'; return
    }
    $lock = $null
    $work = $null
    $completed = $false
    try
    {
        $lock = [IO.File]::Open((Join-Path $root 'installers/.update.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        $headers = @{ 'User-Agent' = 'Sequoia-updater'; 'Accept' = 'application/vnd.github+json' }
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest" -Headers $headers -TimeoutSec 60
        if ($release.draft -or $release.prerelease)
        {
            throw 'Expected a stable published release.'
        }
        $version = Get-Version $release.tag_name
        if ($version -le (Get-Version $info.version))
        {
            Write-Output "Sequoia $( $info.version ) is up to date."; return
        }
        $assets = @($release.assets | Where-Object { $_.name -ceq $info.update_asset })
        $sums = @($release.assets | Where-Object { $_.name -ceq ($info.update_asset + '.sha256') })
        if ($assets.Count -ne 1 -or $sums.Count -ne 1)
        {
            throw "Release is missing the archive/checksum for $( $info.target )."
        }
        if ($Check)
        {
            Write-Output "Update available: $( $info.version ) -> $version"; return
        }
        $work = Join-Path ([IO.Path]::GetTempPath()) ('Sequoia-update-' + [guid]::NewGuid().ToString('N'))
        $null = New-Item -ItemType Directory -Path $work
        $archive = Join-Path $work 'package.zip'
        $checksum = Join-Path $work 'package.sha256'
        foreach ($asset in @($assets[0], $sums[0]))
        {
            $url = [uri]$asset.browser_download_url
            if ($url.Scheme -ne 'https' -or $url.Host -ne 'github.com' -or
                    -not $url.AbsolutePath.StartsWith("/$Repository/releases/download/", [StringComparison]::OrdinalIgnoreCase))
            {
                throw 'Unexpected release download URL.'
            }
        }
        Invoke-WebRequest -UseBasicParsing -Uri $sums[0].browser_download_url -OutFile $checksum -Headers $headers -TimeoutSec 60
        $sum = (Get-Content -LiteralPath $checksum -Raw).Trim()
        if ($sum -notmatch '^([a-fA-F0-9]{64})\s+\*?(.+)$' -or $Matches[2] -cne $info.update_asset)
        {
            throw 'Invalid archive checksum.'
        }
        $expected = $Matches[1]
        Invoke-WebRequest -UseBasicParsing -Uri $assets[0].browser_download_url -OutFile $archive -Headers $headers -TimeoutSec 600
        if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected)
        {
            throw 'Archive SHA-256 mismatch.'
        }
        $source = Expand-Package $archive (Join-Path $work 'expanded')
        $next = Read-Json (Join-Path $source 'docs/build-info.json')
        if ($next.target -cne $info.target -or $next.update_asset -cne $info.update_asset -or
                (Get-Version $next.version) -ne $version)
        {
            throw 'Package version or architecture does not match the release.'
        }
        $newFiles = @(Get-ManagedFiles $next $source)
        foreach ($file in $newFiles)
        {
            if (-not (Test-Path -LiteralPath (Get-PackagePath $source $file) -PathType Leaf))
            {
                throw "Missing package file: $file"
            }
        }
        if ('update-settings.json' -in $newFiles -or '.update.lock' -in $newFiles)
        {
            throw 'Package contains reserved updater files.'
        }
        if (Test-PlayerRunning $root)
        {
            $completed = $true; Write-Output 'Player started during download; update deferred.'; return
        }
        Install-Package $source $root $oldFiles $newFiles (Join-Path $work 'backup')
        try
        {
            Sync-InstalledVersion $root $next.version
        }
        catch
        {
            Write-Warning "Player updated, but the installed-app version could not be refreshed: $_"
        }
        @{ repository = $Repository } | ConvertTo-Json | Set-Content -LiteralPath $configPath -Encoding UTF8
        Write-Output "Updated Sequoia to $version."
        $completed = $true
    }
    finally
    {
        if ($lock)
        {
            $lock.Dispose()
        }
        # Keep staging/backup on failure so a failed rollback remains recoverable.
        if ($work -and $completed)
        {
            $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
            $resolved = [IO.Path]::GetFullPath($work)
            if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
                    (Split-Path $resolved -Leaf) -match '^Sequoia-update-[a-f0-9]{32}$')
            {
                Remove-Item -LiteralPath $resolved -Recurse -Force
            }
        }
        elseif ($work)
        {
            Write-Warning "Update files and backup retained: $work"
        }
    }
}

if ($MyInvocation.InvocationName -ne '.')
{
    try
    {
        if (@($Check, $InstallTask, $RemoveTask).Where({ $_ }).Count -gt 1)
        {
            throw 'Choose only one updater mode.'
        }
        $output = Invoke-Updater
        if (-not $Quiet)
        {
            $output | Write-Output
        }
        exit 0
    }
    catch
    {
        if ($Quiet)
        {
            $_ | Out-String | Add-Content -LiteralPath (Join-Path $PSScriptRoot 'update-error.log')
        }
        else
        {
            Write-Error $_
        }
        exit 1
    }
}
