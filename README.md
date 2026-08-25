# conncheck

Audit the public WiFi or network you just joined.

## What it does

Runs four checks and scores the result Low / Medium / High risk:

- **Security** — WiFi encryption (WPA3/WPA2/Open), DNS leak via DNS-over-HTTPS, captive portal detection
- **Speed** — download, upload, latency, jitter via M-Lab's open NDT7 protocol
- **Reliability** — ping/jitter/packet loss to gateway, 8.8.8.8, and 1.1.1.1
- **Topology** — passive ARP cache (no active scanning)

Pass `--save` to write a full JSON report to `~/.conncheck/reports/`.

conncheck reports what it finds; it doesn't fix it. If a run flags something about the
DNS resolver you're using, [docs/context/dns-hardening.md](docs/context/dns-hardening.md)
covers what that actually means and how to change it.

## Requirements

- Linux
- Node ≥ 24
- `nmcli` (NetworkManager), `ip`, `ping`, `resolvectl`

## Installation

```bash
git clone https://github.com/hamptonmaxwell/conncheck
cd conncheck
npm install
```

That's enough to run it from inside the project (see below). To get a global `conncheck` command on your `PATH`, additionally run `npm link`.

## Usage

From inside the project, no global install needed:

```bash
npx conncheck                       # full audit with terminal output
npm start -- --json                 # equivalent, via the start script
npm start -- --only=topology --save # pass any flag through after `--`
```

After `npm link`, the same flags work as a global command:

```bash
conncheck              # full audit with terminal output
conncheck --json       # JSON to stdout (pipe-friendly)
conncheck --save       # also write the report to ~/.conncheck/reports/
conncheck record       # wrap in asciinema for session recording
```

## Open source

All dependencies carry MIT, Apache 2.0, or ISC licenses. conncheck itself is MIT.

Built on:
- [`ws`](https://github.com/websockets/ws) — MIT (speaks M-Lab's open NDT7 protocol directly for the speed check)
- [`commander`](https://github.com/tj/commander.js) — MIT
- [`axios`](https://github.com/axios/axios) — MIT
- [`chalk`](https://github.com/chalk/chalk) — MIT
- [`ora`](https://github.com/sindresorhus/ora) — MIT

## License

MIT
