$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../scripts/updater.ps1')

function Assert($Condition, $Message)
{
    if (-not $Condition)
    {
        throw $Message
    }
}
function Must-Fail([scriptblock]$Action, $Message)
{
    $failed = $false
    try
    {
        & $Action | Out-Null
    }
    catch
    {
        $failed = $true
    }
    Assert $failed $Message
}
function Put($Root, $Name, $Value)
{
    $path = Join-Path $Root $Name
    $null = New-Item -ItemType Directory -Path (Split-Path $path) -Force
    [IO.File]::WriteAllText($path, $Value)
}
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('Sequoia-updater-test-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testRoot
try
{
    $install = Join-Path $testRoot 'installed'
    $source = Join-Path $testRoot 'new'
    $backup = Join-Path $testRoot 'backup'
    Put $install 'Sequoia.exe' 'old'
    Put $install 'mpv/obsolete.dll' 'obsolete'
    Put $install 'mpv/mpv.conf' 'user setting'
    Put $install 'user.txt' 'personal'
    Put $source 'Sequoia.exe' 'new'
    Put $source 'mpv/new.dll' 'new dependency'
    $old = @('Sequoia.exe', 'mpv/obsolete.dll')
    $new = @('Sequoia.exe', 'mpv/new.dll')
    Install-Package $source $install $old $new $backup
    Assert ((Get-Content (Join-Path $install 'Sequoia.exe')) -eq 'new') 'Executable was not updated'
    Assert (-not (Test-Path (Join-Path $install 'mpv/obsolete.dll'))) 'Stale managed file survived'
    Assert ((Get-Content (Join-Path $install 'mpv/mpv.conf')) -eq 'user setting') 'User config lost'
    Assert ((Get-Content (Join-Path $install 'user.txt')) -eq 'personal') 'User data lost'
    Assert ((Get-Content (Join-Path $backup 'Sequoia.exe')) -eq 'old') 'Backup missing'

    Must-Fail { Install-Package $source $install $new @('Sequoia.exe', 'user.txt') (Join-Path $testRoot 'collision') } 'User file collision accepted'
    $handle = [IO.File]::Open((Join-Path $install 'Sequoia.exe'), 'Open', 'ReadWrite', 'None')
    try
    {
        Must-Fail { Install-Package $source $install $new $new (Join-Path $testRoot 'locked') } 'Locked executable accepted'
    }
    finally
    {
        $handle.Dispose()
    }

    # Force failure after the executable has been copied, then check rollback.
    Put $source 'Sequoia.exe' 'broken-update'
    Must-Fail { Install-Package $source $install $new @('Sequoia.exe', 'zz-missing.dll') (Join-Path $testRoot 'rollback') } 'Missing payload accepted'
    Assert ((Get-Content (Join-Path $install 'Sequoia.exe')) -eq 'new') 'Rollback did not restore executable'
    Assert ((Get-Content (Join-Path $install 'mpv/new.dll')) -eq 'new dependency') 'Rollback did not restore obsolete file'

    foreach ($bad in @('../escape', '/absolute', 'C:/absolute', 'mpv/../../escape', 'mpv\escape', 'file:ads', 'mpv/../file', 'dir./file'))
    {
        Must-Fail { Get-PackagePath $install $bad } "Unsafe path accepted: $bad"
    }
    Assert ((Get-Version 'v1.2.3') -gt (Get-Version '1.2.2')) 'Version comparison failed'
    Assert ((Get-Version '1.10.0') -gt (Get-Version '1.9.0')) 'Numeric version comparison failed'
    Must-Fail { Get-Version 'v1.2.3-rc1' } 'Prerelease version accepted'

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipPath = Join-Path $testRoot 'malicious.zip'
    $zip = [IO.Compression.ZipFile]::Open($zipPath, 'Create')
    $null = $zip.CreateEntry('../escape.txt')
    $zip.Dispose()
    Must-Fail { Expand-Package $zipPath (Join-Path $testRoot 'extracted') } 'ZIP traversal accepted'
    $validZip = Join-Path $testRoot 'valid.zip'
    $zip = [IO.Compression.ZipFile]::Open($validZip, 'Create')
    $entry = $zip.CreateEntry('package/Sequoia.exe')
    $writer = New-Object IO.StreamWriter($entry.Open())
    $writer.Write('fixture')
    $writer.Dispose()
    $zip.Dispose()
    $expanded = Expand-Package $validZip (Join-Path $testRoot 'valid-expanded')
    Assert (Test-Path (Join-Path $expanded 'Sequoia.exe')) 'Valid ZIP extraction failed'
    $networkRoot = Join-Path $testRoot 'network'
    $payload = Join-Path $testRoot 'payload'
    $assetName = 'Sequoia-windows-2022-x86_64-pc-windows-msvc.zip'
    $files = @('Sequoia.exe', 'docs/build-info.json', 'installers/updater.ps1', 'installers/update.cmd')
    $info = @{
        version = '1.0.0'; target = 'x86_64-pc-windows-msvc'; update_asset = $assetName
        update_repository = 'owner/sequoia-mpv-player'; managed_files = $files
    }
    foreach ($folder in @($networkRoot, $payload))
    {
        Put $folder 'Sequoia.exe' 'version 1'
        Put $folder 'installers/update.cmd' 'fixture'
        Put $folder 'docs/build-info.json' ($info | ConvertTo-Json)
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot '../scripts/updater.ps1') -Destination (Join-Path $folder 'installers/updater.ps1')
    }
    Put $networkRoot 'user.txt' 'preserve me'
    $info.version = '1.1.0'
    Put $payload 'docs/build-info.json' ($info | ConvertTo-Json)
    Put $payload 'Sequoia.exe' 'version 2'
    $download = Join-Path $testRoot 'download.zip'
    $zip = [IO.Compression.ZipFile]::Open($download, 'Create')
    foreach ($file in $files)
    {
        $null = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $payload $file), ('package/' + $file))
    }
    $zip.Dispose()
    $script:fixtureArchive = $download
    $script:fixtureSum = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash + '  ' + $assetName
    $script:fixtureVersion = 'v1.1.0'
    $script:fixtureMissingAsset = $false
    $script:fixtureOffline = $false
    function Invoke-RestMethod
    {
        if ($script:fixtureOffline)
        {
            throw 'Fixture: network unavailable'
        }
        $assets = @(
            @{ name = $assetName; browser_download_url = "https://github.com/owner/sequoia-mpv-player/releases/download/v1.1.0/$assetName" },
            @{ name = ($assetName + '.sha256'); browser_download_url = "https://github.com/owner/sequoia-mpv-player/releases/download/v1.1.0/$assetName.sha256" }
        )
        if ($script:fixtureMissingAsset)
        {
            $assets = @()
        }
        @{ tag_name = $script:fixtureVersion; draft = $false; prerelease = $false; assets = $assets }
    }
    function Invoke-WebRequest($Uri, $OutFile)
    {
        if ( $Uri.EndsWith('.sha256'))
        {
            [IO.File]::WriteAllText($OutFile, $script:fixtureSum)
        }
        else
        {
            Copy-Item -LiteralPath $script:fixtureArchive -Destination $OutFile
        }
    }
    function Test-PlayerRunning
    {
        return $false
    }
    . (Join-Path $networkRoot 'installers/updater.ps1')
    function Sync-InstalledVersion($Root, $Version)
    {
        $script:installedVersion = $Version
    }
    Assert ((Get-ManagedFiles (Read-Json (Join-Path $networkRoot 'docs/build-info.json')) $networkRoot).Count -eq $files.Count) 'Nested manifest rejected'
    Must-Fail { Get-ManagedFiles @{ managed_files = $files + @('installers/update-settings.json') } $networkRoot } 'Reserved nested settings accepted'
    $Check = $true
    $output = Invoke-Updater
    Assert ($output -match 'Update available') 'Check mode failed'
    Assert ((Get-Content (Join-Path $networkRoot 'Sequoia.exe')) -eq 'version 1') 'Check mode changed executable'
    $Check = $false
    $script:fixtureOffline = $true
    Must-Fail { Invoke-Updater } 'Network error was ignored'
    Assert ((Get-Content (Join-Path $networkRoot 'Sequoia.exe')) -eq 'version 1') 'Network failure changed executable'
    $script:fixtureOffline = $false
    $script:fixtureMissingAsset = $true
    Must-Fail { Invoke-Updater } 'Missing release assets accepted'
    $script:fixtureMissingAsset = $false
    $correctSum = $script:fixtureSum
    $script:fixtureSum = ('0' * 64) + '  ' + $assetName
    Must-Fail { Invoke-Updater } 'Bad checksum accepted'
    Assert ((Get-Content (Join-Path $networkRoot 'Sequoia.exe')) -eq 'version 1') 'Checksum failure changed executable'
    $script:fixtureSum = $correctSum
    Invoke-Updater | Out-Null
    Assert ((Get-Content (Join-Path $networkRoot 'Sequoia.exe')) -eq 'version 2') 'Full update failed'
    Assert ($script:installedVersion -eq '1.1.0') 'Installed app version not refreshed'
    Assert ((Get-Content (Join-Path $networkRoot 'user.txt')) -eq 'preserve me') 'Full update lost user file'
    $rootFiles = @(Get-ChildItem -LiteralPath $networkRoot -File | Where-Object Name -ne 'user.txt')
    Assert ($rootFiles.Count -eq 1 -and $rootFiles[0].Name -eq 'Sequoia.exe') 'Updater left support files in root'
    $output = Invoke-Updater
    Assert ($output -match 'up to date') 'Same version was installed again'
    $script:fixtureVersion = 'v1.0.0'
    $output = Invoke-Updater
    Assert ($output -match 'up to date') 'Downgrade was accepted'
    function Test-PlayerRunning
    {
        return $true
    }
    $output = Invoke-Updater
    Assert ($output -match 'deferred') 'Running player did not defer update'
    function New-ScheduledTaskAction($Execute, $Argument)
    {
        @{ Execute = $Execute; Argument = $Argument }
    }
    function New-ScheduledTaskTrigger
    {
        'trigger fixture'
    }
    function New-ScheduledTaskSettingsSet
    {
        'settings fixture'
    }
    function New-ScheduledTaskPrincipal
    {
        'principal fixture'
    }
    function Register-ScheduledTask($TaskName, $Action)
    {
        $script:registeredTask = $TaskName; $script:registeredAction = $Action
    }
    function Unregister-ScheduledTask($TaskName)
    {
        $script:removedTask = $TaskName
    }
    $InstallTask = $true
    Invoke-Updater | Out-Null
    Assert ($script:registeredAction.Argument.Contains('-Quiet')) 'Scheduled task is not quiet'
    Assert ($script:registeredAction.Argument.Contains((Join-Path $networkRoot 'installers/updater.ps1'))) 'Scheduled task points at wrong installation'
    Assert ((Read-Json (Join-Path $networkRoot 'installers/update-settings.json')).repository -eq 'owner/sequoia-mpv-player') 'Repository setting was not saved'
    $InstallTask = $false
    $RemoveTask = $true
    Invoke-Updater | Out-Null
    Assert ($script:removedTask -eq $script:registeredTask) 'Wrong scheduled task removed'
    Write-Output 'Updater tests passed: replacement, stale files, user files, locks, rollback, paths, versions and ZIP extraction.'
}
finally
{
    $resolved = [IO.Path]::GetFullPath($testRoot)
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if ($resolved.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase) -and
            (Split-Path $resolved -Leaf) -match '^Sequoia-updater-test-[a-f0-9]{32}$')
    {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
