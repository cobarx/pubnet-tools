# Test the pubnetdiag --repair flow against a WPA2/WPA3 transition-mode AP.
#
# Designed for the scenario where you are connected to a backup network (e.g. Galaxy S23)
# and the target AP (e.g. attinternet) is in range but not your current connection.
#
# Safety timer: if after $SafetyTimeoutSeconds we are not connected to the target
# or the backup, the timer fires and reconnects to $BackupSsid automatically.
#
# Usage:
#   .\scripts\test_repair.ps1 -TargetSsid attinternet
#   .\scripts\test_repair.ps1 -TargetSsid attinternet -BackupSsid "Galaxy S23" -SafetyTimeoutSeconds 120

param(
    [Parameter(Mandatory)] [string] $TargetSsid,
    [string] $BackupSsid           = "Galaxy S23",
    [int]    $SafetyTimeoutSeconds = 120
)

# ── logging setup ─────────────────────────────────────────────────────────────

$LogDir  = Join-Path $PSScriptRoot "..\logs"
$null    = New-Item -ItemType Directory -Force $LogDir
$LogFile = Join-Path $LogDir "repair_test_$(Get-Date -Format 'yyyyMMdd_HHmmss').log"
Start-Transcript -Path $LogFile -Append

function Log {
    param([string]$Msg, [string]$Level = "INFO")
    $line = "[$(Get-Date -Format 'HH:mm:ss')] [$Level] $Msg"
    Write-Host $line
}

function Log-WifiState {
    param([string]$Label)
    $info = netsh wlan show interfaces
    $ssid  = ($info | Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } | Select-Object -First 1)
    $state = ($info | Where-Object { $_ -match '^\s+State\s+:\s+' } | Select-Object -First 1)
    $signal= ($info | Where-Object { $_ -match '^\s+Signal\s+:\s+' } | Select-Object -First 1)
    Log "$Label wifi state:"
    Log "  $($ssid  -replace '^\s+','')"
    Log "  $($state -replace '^\s+','')"
    Log "  $($signal -replace '^\s+','')"
}

function Get-CurrentSsid {
    $line = netsh wlan show interfaces |
        Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } |
        Select-Object -First 1
    if ($line -match '^\s+SSID\s+:\s+(.+)$') { return $matches[1].Trim() }
    return $null
}

# ── preflight ─────────────────────────────────────────────────────────────────

Log "═══════════════════════════════════════════════════"
Log "pubnetdiag --repair test"
Log "Target SSID:      '$TargetSsid'"
Log "Backup SSID:      '$BackupSsid'"
Log "Safety timeout:   ${SafetyTimeoutSeconds}s"
Log "Log file:         $LogFile"
Log "═══════════════════════════════════════════════════"

Log-WifiState "Preflight"

$currentSsid = Get-CurrentSsid
if ($currentSsid -ne $BackupSsid) {
    Log "WARNING: Not on backup '$BackupSsid' (on '$currentSsid'). Safety timer will still reconnect to '$BackupSsid' on failure." "WARN"
}

# ── pass 1: scan ──────────────────────────────────────────────────────────────

Log ""
Log "─── Pass 1: scan ───────────────────────────────────"
Log "Running: pubnetdiag $TargetSsid"

$scanLines = @()
cargo run -p pubnetdiag --bin pubnetdiag -- $TargetSsid 2>&1 | ForEach-Object {
    Log "  $_"
    $scanLines += $_
}

$transitionDetected = $scanLines | Where-Object { $_ -match 'transition|WPA2.WPA3|SaeTransition|⚠' }

if ($transitionDetected) {
    Log "Transition mode detected in scan output." "OK"
} else {
    Log "Transition mode NOT detected in scan output." "WARN"
    Log "Expected WPA2/WPA3 transition mode on '$TargetSsid'. Is this the right AP?" "WARN"
    $proceed = Read-Host "[$(Get-Date -Format 'HH:mm:ss')] Proceed to --repair anyway? (y/N)"
    if ($proceed -ne 'y') {
        Log "Aborted by user." "INFO"
        Stop-Transcript; exit 2
    }
}

# ── arm safety timer ──────────────────────────────────────────────────────────

Log ""
Log "Arming safety timer: will reconnect to '$BackupSsid' in ${SafetyTimeoutSeconds}s if not connected to target or backup."

$safetyJob = Start-Job -ScriptBlock {
    param($targetSsid, $backupSsid, $timeout, $logFile)
    Start-Sleep $timeout
    $line = netsh wlan show interfaces |
        Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } |
        Select-Object -First 1
    $ssid = if ($line -match '^\s+SSID\s+:\s+(.+)$') { $matches[1].Trim() } else { $null }
    $ts = Get-Date -Format 'HH:mm:ss'
    Add-Content $logFile "[$ts] [SAFETY] Timer fired. Current SSID: '$ssid'"
    if ($ssid -ne $targetSsid -and $ssid -ne $backupSsid) {
        Add-Content $logFile "[$ts] [SAFETY] Not on target or backup — reconnecting to '$backupSsid'"
        netsh wlan connect name=$backupSsid | Out-Null
        Start-Sleep 5
        $line2 = netsh wlan show interfaces |
            Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } |
            Select-Object -First 1
        $ssid2 = if ($line2 -match '^\s+SSID\s+:\s+(.+)$') { $matches[1].Trim() } else { $null }
        Add-Content $logFile "[$ts] [SAFETY] After reconnect: '$ssid2'"
    } else {
        Add-Content $logFile "[$ts] [SAFETY] On '$ssid' — no action needed"
    }
} -ArgumentList $TargetSsid, $BackupSsid, $SafetyTimeoutSeconds, $LogFile

# ── pass 2: repair ────────────────────────────────────────────────────────────

Log ""
Log "─── Pass 2: repair ──────────────────────────────────"
Log "Running: pubnetdiag $TargetSsid --repair"
Log "(Enter the passphrase for '$TargetSsid' when prompted.)"
Log ""

Log-WifiState "Before repair"

cargo run -p pubnetdiag --bin pubnetdiag -- $TargetSsid --repair
$repairExitCode = $LASTEXITCODE

Log ""
Log "pubnetdiag exited with code: $repairExitCode"

# ── result ────────────────────────────────────────────────────────────────────

Start-Sleep -Seconds 5
Log-WifiState "After repair"
$afterSsid = Get-CurrentSsid

if ($afterSsid -eq $TargetSsid) {
    Log "SUCCESS: Connected to '$afterSsid' after repair." "OK"
    Stop-Job $safetyJob -ErrorAction SilentlyContinue
    Remove-Job $safetyJob -Force -ErrorAction SilentlyContinue
    Log "Safety timer cancelled."
} elseif ($afterSsid -eq $BackupSsid) {
    Log "RESULT: Still on backup '$afterSsid'. Repair did not switch connection." "WARN"
    Log "Exit code was $repairExitCode — check log above for errors." "WARN"
    Stop-Job $safetyJob -ErrorAction SilentlyContinue
    Remove-Job $safetyJob -Force -ErrorAction SilentlyContinue
} else {
    Log "RESULT: On '$afterSsid' — not target or backup. Waiting for safety timer..." "WARN"
    Wait-Job $safetyJob -Timeout ($SafetyTimeoutSeconds + 15) | Out-Null
    Remove-Job $safetyJob -Force -ErrorAction SilentlyContinue
    $finalSsid = Get-CurrentSsid
    Log "After safety timer: connected to '$finalSsid'"
}

Log ""
Log "═══════════════════════════════════════════════════"
Log "Test complete. Full log: $LogFile"
Log "═══════════════════════════════════════════════════"
Stop-Transcript
