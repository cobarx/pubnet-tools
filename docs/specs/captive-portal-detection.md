---
template_version: 1.0.0
slug: captive-portal-detection
status: agreed
owner: hampton
date: 2026-08-24
related: []
---

# Spec: Captive portal detection

## Intent

conncheck tells the person running it whether the network is intercepting their traffic
behind a captive portal (the login/terms page many public WiFi and hotel networks
inject before allowing real internet access), so they understand why other checks
(speed, DNS leak) might be reading traffic that never left the portal.

**Not in scope:** logging into or navigating a captive portal; distinguishing between
different portal vendors; retrying once a portal is presumed passed.

## Terms

- **Canary request** — a request to a well-known resource with a known, unmodified
  expected response, used specifically to detect interception. Not a general
  connectivity check.
- **Interception** — the network returns something other than the canary's expected
  response: a redirect elsewhere, or a substituted body at the same status.

## Scenarios

### S1 — No captive portal present

**Happy path.**

- **Given** a network connection with no captive portal intercepting traffic
- **When** conncheck requests the canary resource
- **Then** the response matches the canary's expected, unmodified result
- **And** captive portal detected is false
- **And** the detection method is `none`

### S2 — Captive portal redirects to a login page

**Failure.**

- **Given** a network with a captive portal that redirects unauthenticated traffic
- **When** conncheck requests the canary resource
- **Then** the response is a redirect away from the canary's expected destination
- **And** captive portal detected is true
- **And** the detection method is `redirect`
- **And** the redirect's destination is recorded in the result

### S3 — Captive portal substitutes content without redirecting

**Edge.**

- **Given** a network with a captive portal that returns the canary's expected status
  code but substitutes different content in the body
- **When** conncheck requests the canary resource
- **Then** the response body does not match the canary's expected content
- **And** captive portal detected is true
- **And** the detection method is `content-mismatch`

## Open questions

None outstanding.

## Done when

- [ ] `S1` holds: an unmodified canary response yields `detected: false`, `method: 'none'`
- [ ] `S2` holds: a redirect yields `detected: true`, `method: 'redirect'`, and captures
      the redirect destination
- [ ] `S3` holds: a same-status content substitution yields `detected: true`,
      `method: 'content-mismatch'`
- [ ] A canary request that fails outright (no response at all) does not get reported
      as `detected: true` — that's a connectivity failure, not portal interception

## Why this behavior

Captive portals are common enough on public WiFi that other checks would silently
misread portal responses as real network behavior without this. Redirect and
content-substitution are the two mechanisms portals actually use; both need distinct
handling because a caller reading `redirectLocation` shouldn't have to guess whether it
was populated.
