# Simulate an unfixed machine connecting to attinternet, then repair it.
#
# Step 1 -- delete any saved attinternet profile (clean/unfixed state)
# Step 2 -- pubnetdiag attinternet: detect the issue
# Step 3 -- pubnetdiag attinternet --repair: install the WPA2-PSK fix and connect
# Step 4 -- verify connected
#
# Safety timer reconnects to $BackupSsid if the machine is left disconnected.
#
# Usage: powershell.exe -File scripts/diagnose_attinternet.ps1

param(
    [string] $TargetSsid           = "attinternet",
    [string] $BackupSsid           = "Galaxy S23",
    [int]    $SafetyTimeoutSeconds  = 120
)

$LogDir  = Join-Path $PSScriptRoot "..\logs"
$null    = New-Item -ItemType Directory -Force $LogDir
$LogFile = Join-Path $LogDir "diagnose_attinternet_$(Get-Date -Format 'yyyyMMdd_HHmmss').log"
Start-Transcript -Path $LogFile -Append

function Log {
    param([string]$Msg, [string]$Level = "INFO")
    Write-Host "[$(Get-Date -Format 'HH:mm:ss')] [$Level] $Msg"
}

function Get-CurrentSsid {
    $line = netsh wlan show interfaces |
        Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } |
        Select-Object -First 1
    if ($line -match '^\s+SSID\s+:\s+(.+)$') { return $matches[1].Trim() }
    return $null
}

function Log-WifiState {
    param([string]$Label)
    $info   = netsh wlan show interfaces
    $ssid   = $info | Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } | Select-Object -First 1
    $state  = $info | Where-Object { $_ -match '^\s+State\s+:\s+' }  | Select-Object -First 1
    $signal = $info | Where-Object { $_ -match '^\s+Signal\s+:\s+' } | Select-Object -First 1
    Log "${Label}:"
    Log "  $($ssid   -replace '^\s+','')"
    Log "  $($state  -replace '^\s+','')"
    Log "  $($signal -replace '^\s+','')"
}

# ── preflight ─────────────────────────────────────────────────────────────────

Log "======================================================="
Log "pubnetdiag attinternet -- diagnose and repair"
Log "Target:  '$TargetSsid'"
Log "Backup:  '$BackupSsid'"
Log "Log:     $LogFile"
Log "======================================================="
Log-WifiState "Preflight"

# ── step 1: clean state ───────────────────────────────────────────────────────

Log ""
Log "--- Step 1: simulate unfixed machine ---"
$profiles = netsh wlan show profiles 2>&1
if ($profiles -match [regex]::Escape($TargetSsid)) {
    Log "Deleting saved '$TargetSsid' profile..."
    netsh wlan delete profile name=$TargetSsid 2>&1 | ForEach-Object { Log "  $_" }
    Log "Deleted. Machine is now in unfixed state." "OK"
} else {
    Log "No saved '$TargetSsid' profile found -- already in unfixed state." "OK"
}

# ── step 2: diagnose ──────────────────────────────────────────────────────────

Log ""
Log "--- Step 2: diagnose ---"
cargo run -p pubnetdiag --bin pubnetdiag -- $TargetSsid 2>&1 | ForEach-Object { Log "  $_" }

# ── arm safety timer ──────────────────────────────────────────────────────────

$safetyJob = Start-Job -ScriptBlock {
    param($backup, $t, $logFile)
    Start-Sleep $t
    $line = netsh wlan show interfaces |
        Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } |
        Select-Object -First 1
    $ssid = if ($line -match '^\s+SSID\s+:\s+(.+)$') { $matches[1].Trim() } else { $null }
    $ts = Get-Date -Format 'HH:mm:ss'
    Add-Content $logFile "[$ts] [SAFETY] Timer fired. SSID: '$ssid'"
    if ($ssid -ne $backup) {
        Add-Content $logFile "[$ts] [SAFETY] Not on backup -- reconnecting to '$backup'"
        netsh wlan connect name=$backup | Out-Null
        Start-Sleep 5
        $line2 = netsh wlan show interfaces |
            Where-Object { $_ -match '^\s+SSID\s+:\s+' -and $_ -notmatch 'BSSID' } |
            Select-Object -First 1
        $ssid2 = if ($line2 -match '^\s+SSID\s+:\s+(.+)$') { $matches[1].Trim() } else { $null }
        Add-Content $logFile "[$ts] [SAFETY] After reconnect: '$ssid2'"
    } else {
        Add-Content $logFile "[$ts] [SAFETY] On '$ssid' -- no action needed"
    }
} -ArgumentList $BackupSsid, $SafetyTimeoutSeconds, $LogFile

Log ""
Log "Safety timer armed: reconnect to '$BackupSsid' in ${SafetyTimeoutSeconds}s if disconnected."

# ── step 3: repair ────────────────────────────────────────────────────────────

Log ""
Log "--- Step 3: repair ---"
Log "(Enter passphrase for '$TargetSsid' when prompted.)"
Log ""

cargo run -p pubnetdiag --bin pubnetdiag -- $TargetSsid --repair
$repairExit = $LASTEXITCODE
Log "pubnetdiag --repair exited: $repairExit"

# ── step 4: verify ────────────────────────────────────────────────────────────

Log ""
Log "--- Step 4: verify ---"
Start-Sleep -Seconds 5
Log-WifiState "After repair"
$final = Get-CurrentSsid

if ($final -eq $TargetSsid) {
    Log "SUCCESS: Connected to '$TargetSsid'." "OK"
    Stop-Job $safetyJob -ErrorAction SilentlyContinue
    Remove-Job $safetyJob -Force -ErrorAction SilentlyContinue
} else {
    Log "Not connected to '$TargetSsid' (on: '$final')." "WARN"
    Log "Check step 2 output above -- if scan showed WPA2-PSK instead of Transition," "WARN"
    Log "the RSN IE parser may be misclassifying this AP." "WARN"
    Wait-Job $safetyJob -Timeout ($SafetyTimeoutSeconds + 15) | Out-Null
    Remove-Job $safetyJob -Force -ErrorAction SilentlyContinue
    $recovered = Get-CurrentSsid
    Log "After safety timer: '$recovered'"
}

Log ""
Log "======================================================="
Log "Full log: $LogFile"
Log "======================================================="
Stop-Transcript
