pub const OK: i32 = 0;
pub const TRANSITION_FOUND: i32 = 1;
pub const REPAIR_FAILED: i32 = 2;
pub const NO_ADAPTER: i32 = 3;
pub const USAGE_ERROR: i32 = 4;

pub const TABLE: &[(i32, &str, &str)] = &[
    (OK,               "OK",               "Scan clean; no actionable issues. With --repair: fix applied or not needed."),
    (TRANSITION_FOUND, "TRANSITION_FOUND", "Transition-mode AP detected. Run --repair <SSID> to apply the WPA2-PSK workaround."),
    (REPAIR_FAILED,    "REPAIR_FAILED",    "Repair failed: wrong passphrase, connection timed out, or WLAN API error."),
    (NO_ADAPTER,       "NO_ADAPTER",       "No Wi-Fi adapter found or the adapter is disabled."),
    (USAGE_ERROR,      "USAGE_ERROR",      "Invalid invocation (e.g. --repair without a target SSID)."),
];
