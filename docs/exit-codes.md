# pubnetdiag exit codes

| Code | Name | Meaning |
|------|------|---------|
| 0 | `OK` | Scan clean; no actionable issues. With --repair: fix applied or not needed. |
| 1 | `TRANSITION_FOUND` | Transition-mode AP detected. Run --repair <SSID> to apply the WPA2-PSK workaround. |
| 2 | `REPAIR_FAILED` | Repair failed: wrong passphrase, connection timed out, or WLAN API error. |
| 3 | `NO_ADAPTER` | No Wi-Fi adapter found or the adapter is disabled. |
| 4 | `USAGE_ERROR` | Invalid invocation (e.g. --repair without a target SSID). |
