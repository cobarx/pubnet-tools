# NAT Traversal: How Tailscale Punches Through

> The core problem: your router only allows inbound packets if it created an
> outbound entry first. No entry → packet silently dropped. Two machines
> behind separate NATs can't initiate toward each other — both routers block
> the other's first packet.
>
> **The solution: make both routers think they initiated.**

---

## Step 1 — Discover External Addresses via STUN

Each device contacts a STUN server. STUN's only job is to reflect back what
the outside world sees:

```
Your laptop (192.168.1.5:51820)
     │
     │── UDP to STUN server ──────────────────────────────▶
     │◀── "I see you as 73.x.x.x:41000" ─────────────────
```

Your router assigned port `41000` on `73.x.x.x` for this outbound packet.
Tailscale's coordination server collects both devices' external addresses and
passes them to each other.

---

## Step 2 — Simultaneous Send

Tailscale signals both peers to fire UDP packets at each other at the same
time:

```
Your laptop  sends to  98.x.x.x:52000   (Spectre's external addr)
Spectre      sends to  73.x.x.x:41000   (your external addr)
```

The key event happens at your router the moment you send outbound to
`98.x.x.x:52000` — your router writes a NAT table entry:

```
internal 192.168.1.5:51820
    ↔  external 73.x.x.x:41000
    →  destination 98.x.x.x:52000
```

That entry means: *"if I see an inbound packet from `98.x.x.x:52000`,
forward it to `192.168.1.5:51820`."*

---

## Step 3 — The Hole Opens

Spectre's packet arrives at your router from `98.x.x.x:52000`. Your router
checks its NAT table, finds a matching entry, and forwards the packet inbound.
Your router thinks this is a reply to something you sent — it has no idea both
packets were sent simultaneously.

The same happens in reverse at Spectre's router.

```
Your router NAT table:
  192.168.1.5:51820 ↔ 73.x.x.x:41000 ↔ 98.x.x.x:52000
                                                   ↑
                                            MATCH → forward inbound ✓

Spectre's router NAT table:
  192.168.1.x:PORT ↔ 98.x.x.x:52000 ↔ 73.x.x.x:41000
                                                  ↑
                                           MATCH → forward inbound ✓
```

Both NATs have been tricked. A bidirectional UDP tunnel now exists. The
WireGuard Noise handshake runs through it normally.

---

## Why Timing Matters

Your outbound packet must create the NAT entry **before** Spectre's packet
arrives at your router — otherwise the router sees an unknown inbound source
and drops it.

Tailscale coordinates the simultaneous send precisely for this reason. Both
peers are signaled at the same moment through the coordination server.

---

## Where It Breaks: Symmetric NAT

Most home routers use **port-restricted cone NAT** — one external port per
internal source port, reused across destinations. Symmetric NAT assigns a
*different* external port for each different destination:

```
Cone NAT (predictable):
  laptop:51820 → STUN server   =  73.x.x.x:41000
  laptop:51820 → Spectre       =  73.x.x.x:41000  ← same port ✓

Symmetric NAT (unpredictable):
  laptop:51820 → STUN server   =  73.x.x.x:41000
  laptop:51820 → Spectre       =  73.x.x.x:41873  ← different port ✗
```

Spectre was told to expect you at `:41000` but you're arriving at `:41873`.
The hole punch fails because Spectre's router has no entry for that port.

---

## DERP Relay as Fallback

When hole punching fails, both devices make outbound connections to the
nearest DERP (Designated Encrypted Relay for Packets) server. Outbound
connections have no NAT issues.

```
Your laptop ──▶ DERP relay (nearest region) ──▶ Spectre
```

- Still **end-to-end WireGuard encrypted** — DERP sees only ciphertext
- Tailscale continuously retries the direct path in the background
- Upgrades to P2P automatically the moment hole punching succeeds

---

## Full Picture

```
Both devices register external addresses with STUN
          │
          ▼
Tailscale coord server shares addresses with both peers
          │
          ▼
Both peers fire UDP simultaneously at each other's external addr
          │
     ┌────┴────┐
     │         │
  Success   Failure (symmetric NAT / strict firewall)
     │         │
     │         ▼
     │    DERP relay (still encrypted, retries direct in background)
     │         │
     └────┬────┘
          ▼
   Bidirectional UDP path established
          │
          ▼
   WireGuard Noise_IKpsk2 handshake runs over this path
          │
          ▼
   Encrypted tunnel, keys rotate every 180 seconds
```

---

## NAT Types at a Glance

| Type | Hole Punch? | Notes |
|---|---|---|
| Full cone | ✓ Easy | Any host can reach you once mapping exists. Rare. |
| Address-restricted cone | ✓ Works | Only the specific IP you spoke to first. |
| Port-restricted cone | ✓ Works | Same IP and port you spoke to first. Most home routers. |
| Symmetric | ✗ Fails | Different external port per destination. Falls back to DERP. |
