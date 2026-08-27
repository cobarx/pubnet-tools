---
template_version: 1.0.0
epic: wifi-auth-protocol
ticket: 001
slug: wifi-auth-types
type: chore
points: 2
status: todo
tracker_ref: none
pr: none
related: [wifi-auth-protocol-detection#S1, wifi-auth-protocol-detection#S3, wifi-auth-protocol-detection#S4]
---

# Ticket 001: Types + spec wiring

## Goal

Add `WifiAuthProtocol` to `types.rs` and the `auth_protocol` field to `WifiInfo`,
and add the new spec to CLAUDE.md's documentation index. After this lands, every
other ticket in the epic can compile without conflict.

## Scope

- **In:** `WifiAuthProtocol` enum (`Psk`, `Sae`, `SaeTransition`, `Owe`,
  `Enterprise`, `Open`, `Unknown`) in `src/types.rs`; `auth_protocol:
  WifiAuthProtocol` field on `WifiInfo` in `src/platform/mod.rs`, defaulting to
  `Unknown` in existing platform probe return sites; `as_str()` method and serde
  `rename`s following the project convention; CLAUDE.md docs index entry for
  `wifi-auth-protocol-detection`
- **Out:** Any logic that reads or uses `auth_protocol` (that's tickets 2–5);
  changes to `WifiEncryption`

## Acceptance criteria

- `WifiAuthProtocol` compiles, serializes to the correct JSON strings (lowercase-
  kebab: `psk`, `sae`, `sae-transition`, `owe`, `enterprise`, `open`, `unknown`),
  and the `as_str()` method matches the serde form
- `WifiInfo.auth_protocol` is present in the JSON output of `cargo run -- --json`
  (value will be `unknown` until platform probes land, which is correct per
  `wifi-auth-protocol-detection#S4`)
- All existing tests still pass (`cargo test --lib`)

## Notes

Follow the `WifiEncryption` enum's pattern exactly: explicit `#[serde(rename)]`
on each variant, a matching `as_str()` impl. The project convention is that
`{:?}` is never used for user-facing strings.

`WifiInfo` lives in `src/platform/mod.rs`. Every probe that constructs a
`WifiInfo` (linux.rs, macos.rs, windows.rs) will need `auth_protocol:
WifiAuthProtocol::Unknown` added — this is the correct value until those tickets
land and is not a placeholder to remove later.
