# test_diagnose.ps1
# Force a deliberate wrong-passphrase connection failure against a target
# SSID, then run pubnetdiag --diagnose so we can verify the event log
# reader picks up the failure event.
#
# Usage:
#   powershell.exe -File scripts\test_diagnose.ps1 -TargetSsid "Blade Runner 2049"
#   powershell.exe -File scripts\test_diagnose.ps1 -TargetSsid "some-other-net"
#
# The script does NOT need the real passphrase -- it intentionally uses a
# wrong one so the connection fails and generates a WLAN failure event.

param(
    [Parameter(Mandatory=$true)]
    [string]$TargetSsid,

    [string]$WrongPassphrase = "wrong-passphrase-for-testing-1234",

    # Seconds to wait after WlanConnect before running --diagnose.
    # The 4-way handshake timeout on most APs is 2-5 s, so 12 s is safe.
    [int]$WaitSeconds = 12
)

$ErrorActionPreference = "Stop"
$ProfileName = $TargetSsid
$TempXml = [System.IO.Path]::GetTempFileName() + ".xml"

function Write-Step([string]$msg) {
    Write-Host "`n==> $msg" -ForegroundColor Cyan
}

function Cleanup {
    Write-Step "Cleaning up test profile"
    netsh wlan delete profile name="$ProfileName" 2>$null | Out-Null
    if (Test-Path $TempXml) { Remove-Item $TempXml -Force }
}

# --- Step 1: write a WPA2-PSK profile XML with wrong passphrase ----------
Write-Step "Building profile XML for '$TargetSsid' with wrong passphrase"

$XmlContent = @"
<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>$ProfileName</name>
  <SSIDConfig><SSID><name>$TargetSsid</name></SSID></SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>manual</connectionMode>
  <MSM><security>
    <authEncryption>
      <authentication>WPA2PSK</authentication>
      <encryption>AES</encryption>
      <useOneX>false</useOneX>
    </authEncryption>
    <sharedKey>
      <keyType>passPhrase</keyType>
      <protected>false</protected>
      <keyMaterial>$WrongPassphrase</keyMaterial>
    </sharedKey>
  </security></MSM>
</WLANProfile>
"@

[System.IO.File]::WriteAllText($TempXml, $XmlContent, [System.Text.Encoding]::UTF8)
Write-Host "  Profile XML written to $TempXml"

# --- Step 2: install the profile -----------------------------------------
Write-Step "Installing profile via netsh"
$addOut = netsh wlan add profile filename="$TempXml" user=current 2>&1
Write-Host "  $addOut"

# --- Step 3: attempt connection (will fail) ------------------------------
Write-Step "Attempting connection (expecting failure -- wrong passphrase)"
$connectOut = netsh wlan connect name="$ProfileName" ssid="$TargetSsid" 2>&1
Write-Host "  $connectOut"

# --- Step 4: wait for the failure event to land in the WLAN log ----------
Write-Step "Waiting $WaitSeconds s for failure event to be written..."
Start-Sleep -Seconds $WaitSeconds

# --- Step 5: run --diagnose ---------------------------------------------
Write-Step "Running pubnetdiag --diagnose"
$DiagBin = Join-Path $PSScriptRoot "..\target\release\pubnetdiag.exe"
if (-not (Test-Path $DiagBin)) {
    $DiagBin = Join-Path $PSScriptRoot "..\target\debug\pubnetdiag.exe"
}
if (-not (Test-Path $DiagBin)) {
    Write-Host "  pubnetdiag binary not found -- run 'cargo build -p pubnetdiag' first" -ForegroundColor Red
    Cleanup
    exit 1
}

& $DiagBin $TargetSsid --diagnose

# --- Step 6: clean up ----------------------------------------------------
Cleanup
Write-Host "`nDone." -ForegroundColor Green
