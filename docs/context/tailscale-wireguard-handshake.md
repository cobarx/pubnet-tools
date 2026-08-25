# Tailscale + WireGuard: The Handshake

> Two separate but layered processes. Tailscale handles identity and key
> distribution. WireGuard handles the actual tunnel.

---

## Phase 1 — Key Generation

Happens once, locally, when Tailscale starts on a device.

```
private key  →  lives only on this device, never transmitted anywhere
public key   →  safe to share with the world
```

Curve25519 elliptic curve. Knowing the public key tells you nothing about
the private key.

---

## Phase 2 — Registration

```
Your laptop                    Tailscale coordination server
     │                                     │
     │── "here's my public key,            │
     │    here's my auth token (Google)" ──▶
     │                                     │── stores your public key
     │                                     │── associates it with your account
     │◀── "you're in, here are the         │
     │     public keys of all your         │
     │     other devices"                  │
```

The coordination server is a **key directory** — it never sees your private
key. At the end of this phase, your laptop knows Spectre's WireGuard public
key, and Spectre knows yours.

> **This is the proprietary part.** This is also why Tailscale can't read
> your traffic — they only ever held public keys.
> Headscale is a self-hosted open source replacement for this server.

---

## Phase 3 — NAT Traversal

Neither device has a public IP. Tailscale runs STUN-like discovery to find
each device's external address as seen from the internet.

```
Your laptop                    Discovery server
     │                                │
     │── UDP packet ─────────────────▶│
     │◀── "your external address      │
     │     is 73.x.x.x:51820" ────────│
```

The coordination server shares these addresses with both peers. Then both
sides **simultaneously** fire UDP packets at each other's external address.
This "hole punching" tricks both NAT tables into allowing the return traffic:

```
Your laptop (73.x.x.x:51820)       Spectre's router (98.x.x.x:41023)
     │                                          │
     │─── UDP ──▶ 98.x.x.x:41023               │
     │            98.x.x.x:41023 ◀── UDP ───────│
     │                                          │
     │  both NATs now have entries for          │
     │  each other's external address           │
```

Works ~95% of the time. When it fails (symmetric NAT, strict firewalls),
traffic falls back through **DERP relay servers** — still WireGuard-encrypted
end-to-end. The relay sees nothing but ciphertext.

---

## Phase 4 — WireGuard Handshake

Protocol: **Noise\_IKpsk2**
Both devices now know each other's public keys and have a UDP path.
WireGuard executes a **1-RTT handshake** — two messages total.

### Message 1: Initiator → Responder

Your laptop wants to talk to Spectre.

1. Generates a **fresh ephemeral keypair** (different every handshake)
2. `ECDH(ephemeral_private, Spectre_public_key)` → shared secret, never transmitted
3. Uses that secret to **encrypt your static public key**
4. Includes an **encrypted timestamp** (prevents replay attacks)
5. Sends: `[ ephemeral_public_key | encrypted(your_static_key) | encrypted(timestamp) ]`

Spectre can verify this came from a legitimate device because only a device
holding the correct private key could produce a valid ECDH output.

### Message 2: Responder → Initiator

Spectre responds.

1. Generates its own ephemeral keypair
2. Performs two ECDH operations:
   - `ECDH(spectre_ephemeral_private, your_ephemeral_public)`
   - `ECDH(spectre_static_private, your_ephemeral_public)`
3. Combines these into the final **session keys**
4. Sends: `[ spectre_ephemeral_public | empty_encrypted_payload ]`

The empty encrypted payload proves Spectre derived the correct session key
without transmitting the key itself.

```
Handshake complete. Total: 2 UDP packets.
```

Both sides independently compute the same session keys — a send key and a
receive key — without ever transmitting those keys over the wire.

---

## Phase 5 — Data Transport

All subsequent packets:

```
[ 4-byte header | ChaCha20-Poly1305 encrypted payload | 16-byte auth tag ]
```

The auth tag means any tampering causes the packet to be **silently dropped**
— no error messages, no response. This makes WireGuard extremely hard to
fingerprint or probe.

**Session keys rotate every 180 seconds** — forward secrecy. Stealing your
keys tomorrow cannot decrypt sessions recorded today.

---

## Summary

```
ONE TIME:
  Device boots
    → generate WireGuard keypair
    → register public key with Tailscale coordination server

PER CONNECTION:
  1. Coord server distributes peer public keys
  2. NAT traversal: UDP hole punch (DERP relay as fallback)
  3. WireGuard 2-message handshake → session keys derived locally
  4. Traffic: ChaCha20-Poly1305, keys rotate every 3 min

Tailscale servers touch:   public keys + external IP:port only
Your traffic:              Tailscale never sees it
```

---

## Cryptographic Primitives

| Purpose | Algorithm |
|---|---|
| Key exchange | Curve25519 (ECDH) |
| Symmetric encryption | ChaCha20-Poly1305 |
| Hashing | BLAKE2s |
| Handshake framework | Noise\_IKpsk2 |

---

## Why the separation matters

Tailscale solves the hard **human problems**: identity, authentication, NAT,
key distribution.

WireGuard solves the hard **cryptography problem**: a secure, auditable
tunnel (~4,000 lines of code vs OpenVPN's ~600,000).

Neither tries to do the other's job.
