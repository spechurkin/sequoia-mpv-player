$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../scripts/windows-integration.ps1')

function Assert($Condition, $Message)
{
    if (-not $Condition)
    {
        throw $Message
    }
}
function Must-Fail([scriptblock]$Call)
{
    $failed = $false
    try
    {
        & $Call | Out-Null
    }
    catch
    {
        $failed = $true
    }
    Assert $failed 'Expected installation ownership check to fail'
}
$script:registry = @{ }
$script:shortcutTarget = $null
$script:settingsOpened = 0
$script:notifications = 0
$script:removedTasks = @()
function Set-IntegrationValue($Path, $Name, $Value, $Type = 'String')
{
    $script:registry["$Path|$Name"] = $Value
}
function Get-IntegrationValue($Path, $Name)
{
    $script:registry["$Path|$Name"]
}
function Remove-IntegrationValue($Path, $Name)
{
    $script:registry.Remove("$Path|$Name")
}
function Remove-IntegrationKey($Path)
{
    foreach ($key in @($script:registry.Keys))
    {
        if ($key.StartsWith($Path + '|') -or $key.StartsWith($Path + '\'))
        {
            $script:registry.Remove($key)
        }
    }
}
function Get-SequoiaShortcutTarget
{
    $script:shortcutTarget
}
function Set-SequoiaShortcut($Path, $Executable, $Root)
{
    $script:shortcutTarget = $Executable
    $script:shortcutDirectory = $Root
    $script:shortcutPath = $Path
}
function Remove-SequoiaShortcut
{
    $script:shortcutTarget = $null
}
function Open-SequoiaDefaults
{
    $script:settingsOpened++
}
function Send-AssociationChange
{
    $script:notifications++
}
function Remove-SequoiaUpdateTask($Root)
{
    $script:removedTasks += $Root
}

$workspace = Join-Path ([IO.Path]::GetTempPath()) ('Sequoia-integration-test-' + [guid]::NewGuid().ToString('N'))
$root = Join-Path $workspace ("Sequoia O'Brien & " + [char]0x0442 + [char]0x0435 + [char]0x0441 + [char]0x0442)
$programs = Join-Path $workspace 'Programs'
$null = New-Item -ItemType Directory -Path $root -Force
try
{
    $null = New-Item -ItemType Directory -Path (Join-Path $root 'installers'), (Join-Path $root 'docs') -Force
    [IO.File]::WriteAllText((Join-Path $root 'Sequoia.exe'), 'fixture')
    [IO.File]::WriteAllText((Join-Path $root 'installers/windows-file-types.json'), '{"audio":[".mp3",".flac"],"video":[".mkv",".gif"]}')
    [IO.File]::WriteAllText((Join-Path $root 'docs/build-info.json'), '{"version":"1.2.3"}')
    $classes = 'HKCU:\Software\Classes'
    Set-IntegrationValue "$classes\.mp3" '' 'OtherPlayer.mp3'
    Set-IntegrationValue "$classes\.mp3\OpenWithProgids" 'OtherPlayer.mp3' ''
    Set-IntegrationValue "$classes\.mp3\UserChoice" 'ProgId' 'OtherPlayer.mp3'
    Set-IntegrationValue 'HKCU:\Software\Sequoia\Preferences' 'UserSetting' 'keep'
    Invoke-WindowsIntegration Install $root $programs | Out-Null
    $exe = Join-Path $root 'Sequoia.exe'
    Assert ($script:shortcutTarget -eq $exe) 'Shortcut target incorrect'
    Assert ($script:shortcutDirectory -eq $root) 'Shortcut working directory incorrect'
    Assert ($script:shortcutPath -eq (Join-Path $programs 'Sequoia.lnk')) 'Shortcut is not in Programs'
    Assert ($script:settingsOpened -eq 1) 'Default apps settings were not opened'
    Assert ((Get-IntegrationValue "$classes\Sequoia.mp3\shell\open\command" '') -eq ('"' + $exe + '" "%1"')) 'File path quoting incorrect'
    Assert ((Get-IntegrationValue 'HKCU:\Software\Sequoia\Capabilities\FileAssociations' '.mkv') -eq 'Sequoia.mkv') 'Media association missing'
    Assert ((Get-IntegrationValue 'HKCU:\Software\RegisteredApplications' 'Sequoia') -eq 'Software\Sequoia\Capabilities') 'Default apps registration missing'
    $uninstall = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Sequoia'
    Assert ((Get-IntegrationValue $uninstall 'DisplayVersion') -eq '1.2.3') 'Installed version incorrect'
    Assert ((Get-IntegrationValue $uninstall 'UninstallString').Contains('"' + (Join-Path $root 'installers/windows-integration.ps1') + '"')) 'Uninstall path quoting incorrect'
    Assert ((Get-IntegrationValue "$classes\.mp3\UserChoice" 'ProgId') -eq 'OtherPlayer.mp3') 'UserChoice modified'
    Assert ((Get-IntegrationValue "$classes\.mp3" '') -eq 'OtherPlayer.mp3') 'Existing default modified'
    Assert (@($script:registry.Keys | Where-Object { $_ -notlike 'HKCU:\*' }).Count -eq 0) 'Installation wrote outside HKCU'
    $count = $script:registry.Count
    $NoSettings = $true
    Invoke-WindowsIntegration Install $root $programs | Out-Null
    Assert ($script:registry.Count -eq $count) 'Repeated installation added duplicate entries'
    Assert ($script:settingsOpened -eq 1) 'NoSettings was ignored'
    Must-Fail { Invoke-WindowsIntegration Uninstall (Join-Path $workspace 'another-copy') $programs }
    Assert ($script:shortcutTarget -eq $exe) 'Old uninstaller affected current installation'

    [IO.File]::WriteAllText((Join-Path $root 'installers/windows-file-types.json'), '{"audio":[".mp3"],"video":[".mkv",".gif"]}')
    Invoke-WindowsIntegration Install $root $programs | Out-Null
    Assert ($null -eq (Get-IntegrationValue "$classes\.flac\OpenWithProgids" 'Sequoia.flac')) 'Obsolete association survived'
    Invoke-WindowsIntegration Uninstall $root $programs | Out-Null
    Assert ($null -eq $script:shortcutTarget) 'Shortcut survived uninstallation'
    Assert ($script:removedTasks -contains $root) 'Update task was not removed'
    Assert ($null -eq (Get-IntegrationValue "$classes\.mp3\OpenWithProgids" 'Sequoia.mp3')) 'OpenWith registration survived'
    Assert ($null -eq (Get-IntegrationValue $uninstall 'DisplayName')) 'Installed apps entry survived'
    Assert ((Get-IntegrationValue "$classes\.mp3\UserChoice" 'ProgId') -eq 'OtherPlayer.mp3') 'Uninstall changed UserChoice'
    Assert ($script:registry.ContainsKey("$classes\.mp3\OpenWithProgids|OtherPlayer.mp3")) 'Another player registration was deleted'
    Assert ((Get-IntegrationValue 'HKCU:\Software\Sequoia\Preferences' 'UserSetting') -eq 'keep') 'User preferences were deleted'
    Assert (Test-Path -LiteralPath (Join-Path $root 'Sequoia.exe')) 'Player files deleted'
    Invoke-WindowsIntegration Uninstall $root $programs | Out-Null
    Write-Output 'Windows integration tests passed: registration, quoted paths, defaults, shortcut, repeat install, ownership, obsolete types, uninstall and data preservation (isolated registry/shortcut adapters).'
}
finally
{
    $resolved = [IO.Path]::GetFullPath($workspace)
    $temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if ($resolved.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase) -and
            (Split-Path $resolved -Leaf) -match '^Sequoia-integration-test-[a-f0-9]{32}$')
    {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
