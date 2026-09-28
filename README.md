# Reemote

A personal, self-hosted remote desktop tool (AnyDesk-style) for controlling
your own Windows and Mac machines. No third-party servers, no account, no
telemetry — you run both ends yourself.

Two binaries, built from one Rust workspace so the same code runs natively
on Windows and macOS:

- **`reemote-host`** — runs on the machine you want to control. Captures
  the screen, injects mouse/keyboard input, and listens for one incoming
  connection at a time.
- **`reemote-client`** — the GUI you use to view and control a remote
  `reemote-host`.

## How it works

```
crates/
  protocol/   shared wire messages (handshake, auth, frames, input)
  host/       agent: screen capture, input injection, TLS server
  client/     controller: egui GUI, TLS client, input capture
```

- Screen capture: `xcap` grabs frames per-monitor; each frame is diffed
  against the previous one (row-range bounding box) so mostly-static
  screens send far less data. Changed regions are JPEG-encoded and sent.
- Input injection: `enigo` replays mouse moves/clicks/scroll and keyboard
  events on the host, cross-platform.
- Transport: TLS 1.3 over TCP (`rustls`), length-prefixed `bincode`
  messages (`reemote-protocol`).
- Auth: a password you set on the host (Argon2-hashed, never stored or
  sent in plaintext at rest) is required before any input or frame data
  flows.
- Trust model: the host generates a self-signed TLS certificate on first
  run. The client pins the certificate's fingerprint the first time it
  connects to a given `host:port` (trust-on-first-use, the same model SSH
  uses for host keys) and refuses to connect silently if it ever changes.
  The TLS handshake signature is fully verified, so this isn't just
  "accept anything" — it's "accept this specific host, verified
  cryptographically, forever after the first connection."

## Building

Requires a recent Rust toolchain (`rustup`).

```sh
cargo build --release
```

This produces `target/release/reemote-host` and `target/release/reemote-client`
on whichever platform you build on. Build natively on each OS you want to
support — cross-compiling GUI/native-input apps from macOS to Windows (or
vice versa) is unreliable, so build the Windows binaries on a Windows
machine (or CI runner) and the macOS binaries on a Mac. All dependencies
chosen (`xcap`, `enigo`, `rustls`, `eframe`) explicitly support both
platforms; only macOS has been build/run-tested so far in this repo.

## Running

On the machine you want to control (the **host**):

```sh
reemote-host set-password      # first-time setup, prompts for a password
reemote-host run --port 7723   # start listening
```

On startup, `run` prints the TLS fingerprint. Note it down (or run
`reemote-host fingerprint` any time) — the client shows a warning the
first time it pins a new host, so you can cross-check it matches.

On macOS, the host process needs **Screen Recording** permission
(System Settings → Privacy & Security → Screen Recording) and, to inject
input, **Accessibility** permission, both granted to whatever runs
`reemote-host` (Terminal, or the packaged app once you build one).

On the controlling machine, run `reemote-client`, enter the host's
address, port, and password, and click Connect. For access over the
internet (not just LAN), you currently need to forward the host's port
through your router/firewall or connect over a VPN/Tailscale — see
Roadmap below for built-in NAT traversal.

## Security notes

- Change the default test password before real use if you've been trying
  this out locally — `reemote-host set-password`.
- The access password is the only thing standing between the internet and
  full control of the host machine if you port-forward it. Use a strong,
  unique password, and prefer a VPN/Tailscale over raw port-forwarding
  when possible.
- There's no login rate-limiting or lockout yet (see Roadmap) — don't
  expose the port to the open internet without a strong password at
  minimum.

## Roadmap / not yet implemented

Deliberately out of scope for the current MVP, in rough priority order:

1. **NAT traversal / relay server** — right now two machines must be
   directly reachable (same LAN, port-forward, or VPN). A small relay
   service (host and client both dial out to it) would remove that
   requirement, like AnyDesk's ID servers.
2. **Packaging** — `.app`/`.dmg` for macOS, `.msi`/installer for Windows,
   auto-start-on-login option for the host.
3. **Better video pipeline** — current tile/row diffing + JPEG is simple
   and works, but a real video codec (e.g. H.264 via hardware encoders)
   would cut bandwidth and latency substantially for high-motion content.
4. **Login throttling/lockout** after repeated failed passwords.
5. **Clipboard sync**, **file transfer**, **multi-monitor switching in
   the client UI** (host already reports all monitors).
6. Windows-side build/run verification (everything here has only been
   exercised on macOS so far).
