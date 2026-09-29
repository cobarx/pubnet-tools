//! Requests macOS Location Services authorization for this process via
//! CoreLocation directly, so `pubnetchk` can appear in System Settings'
//! Location Services list the way a real `.app` does — normally impossible
//! for a bare CLI binary. Paired with `build.rs`'s embedded Info.plist
//! section, which supplies the usage-description string the authorization
//! prompt needs.
//!
//! spec: docs/specs/permissions-helper.md
//! decision: docs/decisions/2026-09-04-macos-location-authorization-request.md

use core_foundation::runloop::{CFRunLoop, kCFRunLoopDefaultMode};
use objc2_core_location::{CLAuthorizationStatus, CLLocationManager};
use std::time::{Duration, Instant};

pub fn authorization_status() -> CLAuthorizationStatus {
    unsafe { CLLocationManager::new().authorizationStatus() }
}

/// Requests "when in use" authorization and pumps the run loop (there is no
/// `NSApplication`/AppKit loop already running to do this for us) until the
/// status leaves `NotDetermined` or `timeout` elapses. Returns the real,
/// observed status either way — a status still `NotDetermined` after the
/// full timeout is a normal, expected outcome for a non-bundled process, not
/// an error.
pub fn request_when_in_use_authorization(timeout: Duration) -> CLAuthorizationStatus {
    unsafe {
        let manager = CLLocationManager::new();
        let status = manager.authorizationStatus();
        if status != CLAuthorizationStatus::NotDetermined {
            return status;
        }
        manager.requestWhenInUseAuthorization();

        let deadline = Instant::now() + timeout;
        loop {
            CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(100), false);
            let status = manager.authorizationStatus();
            if status != CLAuthorizationStatus::NotDetermined || Instant::now() >= deadline {
                return status;
            }
        }
    }
}

pub fn describe(status: CLAuthorizationStatus) -> &'static str {
    match status {
        CLAuthorizationStatus::NotDetermined => "not determined",
        CLAuthorizationStatus::Restricted => "restricted (blocked by a device policy)",
        CLAuthorizationStatus::Denied => "denied",
        CLAuthorizationStatus::AuthorizedAlways => "authorized",
        CLAuthorizationStatus::AuthorizedWhenInUse => "authorized",
        _ => "unknown",
    }
}
