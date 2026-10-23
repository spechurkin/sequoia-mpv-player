#Requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('Install', 'Uninstall')][string]$Action = 'Install',
    [switch]$NoSettings,
    [switch]$Quiet
)
$ErrorActionPreference = 'Stop'

function Get-IntegrationValue($Path, $Name)
{
    if (Test-Path -LiteralPath $Path)
    {
        $key = Get-Item -LiteralPath $Path
        try
        {
            $key.GetValue($Name, $null)
        }
        finally
        {
            $key.Dispose()
        }
    }
}
function Set-IntegrationValue($Path, $Name, $Value, $Type = 'String')
{
    if (-not $Path.StartsWith('HKCU:\Software\', [StringComparison]::OrdinalIgnoreCase))
    {
        throw 'Installation registration must stay under HKCU Software.'
    }
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($Path.Substring(6))
    try
    {
        if ($Type -eq 'MultiString')
        {
            $Value = [string[]]$Value
        }
        $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::$Type)
    }
    finally
    {
        $key.Dispose()
    }
}
function Remove-IntegrationValue($Path, $Name)
{
    if ($null -ne (Get-IntegrationValue $Path $Name))
    {
        Remove-ItemProperty -LiteralPath $Path -Name $Name
    }
}
function Remove-IntegrationKey($Path)
{
    # All callers supply fixed keys belonging to Sequoia, never extension roots.
    if (Test-Path -LiteralPath $Path)
    {
        Remove-Item -LiteralPath $Path -Recurse -Force
    }
}
function Get-SequoiaShortcutTarget($Path)
{
    if (Test-Path -LiteralPath $Path)
    {
        $shell = New-Object -ComObject WScript.Shell
        try
        {
            $shell.CreateShortcut($Path).TargetPath
        }
        finally
        {
            $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
        }
    }
}
function Set-SequoiaShortcut($Path, $Executable, $Root)
{
    $null = New-Item -ItemType Directory -Path (Split-Path $Path) -Force
    $shell = New-Object -ComObject WScript.Shell
    try
    {
        $shortcut = $shell.CreateShortcut($Path)
        $shortcut.TargetPath = $Executable
        $shortcut.Arguments = ''
        $shortcut.WorkingDirectory = $Root
        $shortcut.IconLocation = $Executable + ',0'
        $shortcut.Description = 'Sequoia media player'
        $shortcut.Save()
    }
    finally
    {
        $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
    }
}
function Remove-SequoiaShortcut($Path)
{
    Remove-Item -LiteralPath $Path -Force
}
function Send-AssociationChange
{
    if (-not ('Sequoia.ShellRegistration' -as [type]))
    {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace Sequoia {
    public static class ShellRegistration {
        [DllImport("shell32.dll")]
        public static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);
    }
}
'@
    }
    [Sequoia.ShellRegistration]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}
function Open-SequoiaDefaults
{
    Start-Process 'ms-settings:defaultapps?registeredAppUser=Sequoia'
}
function Remove-SequoiaUpdateTask($Root)
{
    $sha = [Security.Cryptography.SHA256]::Create()
    try
    {
        $id = ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Root.ToLowerInvariant())))).Replace('-', '').Substring(0, 16)
    }
    finally
    {
        $sha.Dispose()
    }
    $name = "Sequoia Update $id"
    $task = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
    if ($task)
    {
        Unregister-ScheduledTask -TaskName $name -Confirm:$false
    }
}

function Invoke-WindowsIntegration($Mode, $Root, $Programs)
{
    $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    $exe = Join-Path $rootPath 'Sequoia.exe'
    $classes = 'HKCU:\Software\Classes'
    $state = 'HKCU:\Software\Sequoia\Integration'
    $capabilities = 'HKCU:\Software\Sequoia\Capabilities'
    $registered = 'HKCU:\Software\RegisteredApplications'
    $app = "$classes\Applications\Sequoia.exe"
    $appPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\Sequoia.exe'
    $uninstall = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Sequoia'
    $shortcut = Join-Path $Programs 'Sequoia.lnk'
    $previous = Get-IntegrationValue $state 'InstallLocation'
    $oldExtensions = @(Get-IntegrationValue $state 'Extensions')
    $shortcutTarget = Get-SequoiaShortcutTarget $shortcut

    if ($Mode -eq 'Uninstall')
    {
        if ($previous -and $previous -ine $rootPath)
        {
            throw "Another Sequoia installation is registered at $previous. Run its uninstall.cmd."
        }
        if (-not $previous)
        {
            Write-Output 'Sequoia is not registered for this user.'; return
        }
        Remove-SequoiaUpdateTask $rootPath
        foreach ($extension in $oldExtensions)
        {
            if ($extension -notmatch '^\.[a-z0-9]+$')
            {
                throw 'Invalid registered extension.'
            }
            $progId = 'Sequoia' + $extension
            Remove-IntegrationValue "$classes\$extension\OpenWithProgids" $progId
            Remove-IntegrationKey "$classes\$progId"
        }
        if ($shortcutTarget -ieq $exe)
        {
            Remove-SequoiaShortcut $shortcut
        }
        Remove-IntegrationValue $registered 'Sequoia'
        foreach ($key in @($app, $appPath, $capabilities, $uninstall, $state))
        {
            Remove-IntegrationKey $key
        }
        Send-AssociationChange
        Write-Output 'Sequoia registration, Start menu shortcut and automatic update task removed. Player files and personal data were kept.'
        return
    }

    if (-not (Test-Path -LiteralPath $exe -PathType Leaf))
    {
        throw 'Sequoia.exe must be in the parent folder of installers.'
    }
    $types = Get-Content -LiteralPath (Join-Path $rootPath 'installers/windows-file-types.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $extensions = @(@($types.audio) + @($types.video) | Sort-Object -Unique)
    if (-not $extensions.Count)
    {
        throw 'No supported media types found.'
    }
    foreach ($extension in @($extensions) + @($oldExtensions))
    {
        if ($extension -and $extension -notmatch '^\.[a-z0-9]+$')
        {
            throw "Invalid media extension: $extension"
        }
    }
    if ($shortcutTarget -and $shortcutTarget -ine $exe -and
            (-not $previous -or $shortcutTarget -ine (Join-Path $previous 'Sequoia.exe')))
    {
        throw 'A different Start menu shortcut named Sequoia already exists.'
    }
    $version = 'unknown'
    $buildInfo = Join-Path $rootPath 'docs/build-info.json'
    if (Test-Path -LiteralPath $buildInfo)
    {
        $version = (Get-Content -LiteralPath $buildInfo -Raw -Encoding UTF8 | ConvertFrom-Json).version
    }
    $command = '"' + $exe + '" "%1"'
    $icon = '"' + $exe + '",0'
    if ($previous -and $previous -ine $rootPath)
    {
        Remove-SequoiaUpdateTask $previous
    }
    Set-IntegrationValue $state 'InstallLocation' $rootPath
    Set-IntegrationValue $state 'Extensions' $extensions 'MultiString'
    Set-IntegrationValue $appPath '' $exe
    Set-IntegrationValue $appPath 'Path' $rootPath
    Set-IntegrationValue $app 'FriendlyAppName' 'Sequoia'
    Set-IntegrationValue "$app\DefaultIcon" '' $icon
    Set-IntegrationValue "$app\shell\open\command" '' $command
    Set-IntegrationValue "$app\shell\open" 'FriendlyAppName' 'Sequoia'
    Set-IntegrationValue $capabilities 'ApplicationName' 'Sequoia'
    Set-IntegrationValue $capabilities 'ApplicationDescription' 'Sequoia audio and video player powered by mpv'
    Set-IntegrationValue $capabilities 'ApplicationIcon' $icon
    # Remove only Sequoia's obsolete entries; extension defaults belong to Windows.
    foreach ($extension in $oldExtensions)
    {
        if ($extension -and $extension -notin $extensions)
        {
            Remove-IntegrationValue "$classes\$extension\OpenWithProgids" ('Sequoia' + $extension)
            Remove-IntegrationKey "$classes\Sequoia$extension"
            Remove-IntegrationValue "$capabilities\FileAssociations" $extension
            Remove-IntegrationValue "$app\SupportedTypes" $extension
        }
    }
    foreach ($extension in $extensions)
    {
        $progId = 'Sequoia' + $extension
        $description = if ($extension -in $types.audio)
        {
            'audio'
        }
        else
        {
            'video'
        }
        Set-IntegrationValue "$classes\$progId" '' "Sequoia $description file ($extension)"
        Set-IntegrationValue "$classes\$progId" 'FriendlyTypeName' "Sequoia $description file ($extension)"
        Set-IntegrationValue "$classes\$progId\DefaultIcon" '' $icon
        Set-IntegrationValue "$classes\$progId\shell\open\command" '' $command
        Set-IntegrationValue "$classes\$extension\OpenWithProgids" $progId ''
        Set-IntegrationValue "$app\SupportedTypes" $extension ''
        Set-IntegrationValue "$capabilities\FileAssociations" $extension $progId
    }
    Set-IntegrationValue $registered 'Sequoia' 'Software\Sequoia\Capabilities'
    Set-IntegrationValue $uninstall 'DisplayName' 'Sequoia'
    Set-IntegrationValue $uninstall 'DisplayVersion' $version
    Set-IntegrationValue $uninstall 'DisplayIcon' $icon
    Set-IntegrationValue $uninstall 'InstallLocation' $rootPath
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $remove = '"' + $powershell + '" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "' +
            (Join-Path $rootPath 'installers/windows-integration.ps1') + '" -Action Uninstall -Quiet'
    Set-IntegrationValue $uninstall 'UninstallString' $remove
    Set-IntegrationValue $uninstall 'QuietUninstallString' $remove
    Set-IntegrationValue $uninstall 'NoModify' 1 'DWord'
    Set-IntegrationValue $uninstall 'NoRepair' 1 'DWord'
    Set-SequoiaShortcut $shortcut $exe $rootPath
    Send-AssociationChange
    Write-Output 'Sequoia installed for this user. Choose Sequoia for media types in Windows Default apps settings.'
    if (-not $NoSettings -and -not $Quiet)
    {
        Open-SequoiaDefaults
    }
}

if ($MyInvocation.InvocationName -ne '.')
{
    try
    {
        $messages = Invoke-WindowsIntegration $Action (Split-Path $PSScriptRoot -Parent) ([Environment]::GetFolderPath('Programs'))
        if (-not $Quiet)
        {
            $messages | Write-Output
        }
        exit 0
    }
    catch
    {
        Write-Error $_ -ErrorAction Continue
        exit 1
    }
}
