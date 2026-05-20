# PQVPN — Implementation Plan

A post-quantum VPN platform: Windows Tauri client ↔ TLS (pinned cert) ↔ Go server, ML-KEM-768 key exchange produces a 32-byte PSK, WireGuard uses it as PresharedKey.

## User Review Required

> [!IMPORTANT]
> **GCP Server**: Do you already have a GCP VM with a static IP and firewall rules for UDP 51820 + TCP 8443? If not, I can write the provisioning script but you'll need to create the VM yourself.

> [!IMPORTANT]
> **Scope for tonight**: The Word doc describes 3 modes (Server, Peer Relay, Gaming), but the runbook correctly sequences Server Mode first. I recommend we build **Steps 1–6** tonight (full Server Mode with PQ handshake + Tauri UI). Peer Relay and Gaming Mode are clean additions later. Agreed?

> [!WARNING]
> **Self-signed cert**: We need a TLS certificate for the server that the client pins. I'll generate one during server setup using `openssl`. The cert's DER bytes get compiled into the client binary. This means the client binary is tied to one specific server — which is correct for v1.

## Open Questions

1. **Server IP**: Do you have a GCP static IP already, or should I leave it as a config placeholder?
2. **WireGuard for Windows**: Is it already installed on your machine? The client needs `wireguard.exe` at `C:\Program Files\WireGuard\`.
3. **Auth token**: The runbook says "JWT for v1". For tonight, do you want real JWT validation on the server, or a shared secret/API key that we'll upgrade later?
4. **UI framework**: The Word doc mentions React/TS for the Tauri frontend. I'll use React + TypeScript + Vite (Tauri's default). OK?

---

## Proposed Changes

The project will live in `c:\Users\AYUSH KUMAR\Desktop\vpnX\` with this structure:

```
vpnX/
├── client/                          # Tauri desktop app
│   ├── src/                         # React frontend (TypeScript)
│   │   ├── App.tsx                  # Main app with mode selector
│   │   ├── components/
│   │   │   ├── ConnectButton.tsx    # Big connect/disconnect toggle
│   │   │   ├── StatusBadge.tsx      # PQC Active / Off / Degraded
│   │   │   ├── IntegrityMonitor.tsx # Expandable stats panel
│   │   │   ├── ModeSelector.tsx     # Server / Peer / Gaming tabs
│   │   │   └── Settings.tsx         # Config panel
│   │   ├── styles/
│   │   │   └── index.css            # Full design system
│   │   └── lib/
│   │       └── tauri.ts             # Typed invoke wrappers
│   ├── src-tauri/                   # Rust backend
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs              # Tauri entry point
│   │   │   ├── commands.rs          # Tauri IPC commands (connect, disconnect, status)
│   │   │   ├── pqc/
│   │   │   │   ├── mod.rs
│   │   │   │   └── psk_provider.rs  # ML-KEM-768 negotiation (from your file, fixed)
│   │   │   ├── wireguard/
│   │   │   │   ├── mod.rs
│   │   │   │   └── windows.rs       # Tunnel service control (from your file)
│   │   │   └── config.rs            # App config + pinned cert
│   │   └── tauri.conf.json
│   ├── package.json
│   ├── tsconfig.json
│   └── vite.config.ts
│
├── server/                          # Go server daemon
│   ├── go.mod
│   ├── go.sum
│   ├── cmd/
│   │   └── pqvpnd/
│   │       └── main.go              # Entry point, flags, TLS listener
│   ├── internal/
│   │   ├── psk/
│   │   │   └── provider.go          # ML-KEM-768 server side (from your file)
│   │   ├── api/
│   │   │   ├── server.go            # HTTP API: /register, /connect, /disconnect, /status
│   │   │   └── handlers.go          # Route handlers
│   │   ├── peers/
│   │   │   ├── store.go             # SQLite peer registry
│   │   │   └── store_test.go
│   │   └── wg/
│   │       └── manager.go           # wg command wrapper (set peer, remove peer)
│   ├── certs/                       # Generated self-signed cert + key
│   └── pqvpnd.service               # Systemd unit file
│
└── scripts/
    ├── gen_certs.sh                  # Generate self-signed TLS cert
    ├── gen_wg_keys.sh                # Generate WireGuard key pairs
    └── setup_server.sh              # Server provisioning script
```

---

### Phase 1 — Project Scaffold

#### [NEW] client/ (Tauri app)
- Initialize with `npx create-tauri-app` → React + TypeScript + Vite
- Configure `Cargo.toml` with exact dependency versions:
  - `ml-kem = "0.2"` (FIPS 203 pure Rust)
  - `hkdf = "0.12"`, `sha2 = "0.10"` (KDF)
  - `rustls = "0.23"` (TLS with cert pinning)
  - `base64 = "0.22"`, `serde/serde_json`, `anyhow`, `rand = "0.8"`
- Verify `ml-kem` API: run `cargo doc -p ml-kem --open` and confirm `generate`, `encapsulate`, `decapsulate` method names match your `psk_provider.rs`

#### [NEW] server/ (Go module)
- `go mod init github.com/pqvpn/server` with Go 1.24+
- Dependencies: `golang.org/x/crypto` (for HKDF), `modernc.org/sqlite` (pure-Go SQLite)
- Verify `crypto/mlkem` exists in the Go version on the server

#### [NEW] scripts/
- `gen_certs.sh`: `openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 3650 -keyout server.key -out server.crt -subj "/CN=pqvpn"` + export DER for client pinning
- `gen_wg_keys.sh`: `wg genkey | tee server.key | wg pubkey > server.pub` (same for client)

---

### Phase 2 — WireGuard Windows Integration (Step 2 from runbook)

#### [NEW] [windows.rs](file:///c:/Users/AYUSH%20KUMAR/Desktop/vpnX/client/src-tauri/src/wireguard/windows.rs)
Your existing `wireguard_windows.rs` — clean and correct. I'll integrate it as-is with minor fixes:
- Add `pub mod windows;` in `wireguard/mod.rs`
- The `tunnel_up` / `tunnel_down` / `tunnel_is_up` API is exactly right
- `render_conf` correctly includes the `PresharedKey` line

**Gate 2 test**: Rust binary writes a config with hand-generated keys, calls `tunnel_up`, verify with `wg show` on server.

---

### Phase 3 — PQ PSK Negotiation Engine (Step 3 from runbook)

#### [NEW] [psk_provider.rs](file:///c:/Users/AYUSH%20KUMAR/Desktop/vpnX/client/src-tauri/src/pqc/psk_provider.rs)
Your `psk_provider.rs` with **verified API fixes**. The `ml-kem 0.2` crate API needs specific attention:
- `MlKem768::generate(&mut rng)` → returns `(DecapsulationKey, EncapsulationKey)`
- `ek.as_bytes()` → serializes the encapsulation key
- `dk.decapsulate(&ct)` → returns `Result<SharedSecret>`
- `ek.encapsulate(&mut rng)` → returns `(Ciphertext, SharedSecret)` (server side test)

I'll verify exact method signatures against the crate docs and fix any renames.

#### [NEW] [provider.go](file:///c:/Users/AYUSH%20KUMAR/Desktop/vpnX/server/internal/psk/provider.go)
Your `provider.go` — correct. Go 1.24 `crypto/mlkem`:
- `mlkem.NewEncapsulationKey768(ekBytes)` → parse client's encaps key
- `ek.Encapsulate()` → `(sharedSecret, ciphertext)`
- HKDF with same salt + info string → identical PSK

**Gate 3 test**: Both sides log identical base64 PSK prefix. `wg show wg0 preshared-keys` shows it set.

---

### Phase 4 — Server Daemon with HTTP API (Step 5 from runbook)

#### [NEW] server/cmd/pqvpnd/main.go
- Parse flags: `--listen`, `--wg-interface`, `--wg-pubkey`, `--endpoint`, `--cert`, `--key`
- Load TLS cert, start HTTPS listener
- Routes: `POST /connect` (KEM exchange + wg peer add), `DELETE /disconnect` (wg peer remove), `GET /status`

#### [NEW] server/internal/api/server.go + handlers.go
- `/connect` handler: accepts JSON `{ek_b64, wg_pubkey, auth_token}`, runs the KEM exchange, returns `{ct_b64, server_wg_pubkey, server_endpoint, hkdf_salt_b64}`
- This replaces the raw TCP protocol from `provider.go` with a proper HTTP endpoint (same crypto, better API)
- `/disconnect`: removes WireGuard peer
- `/status`: returns server health + connected peers count

#### [NEW] server/internal/peers/store.go
- SQLite table: `peers(wg_pubkey TEXT PRIMARY KEY, allowed_ip TEXT, connected_at DATETIME, last_handshake DATETIME)`
- IP assignment: simple incrementing from 10.8.0.2/32

#### [NEW] server/internal/wg/manager.go
- Wraps `wg set` / `wg set peer ... remove` commands
- PSK passed via stdin (never in argv)

---

### Phase 5 — PSK Rotation (Step 4 from runbook)

#### [MODIFY] psk_provider.rs
- Add `async fn rotation_loop(interval: Duration)` — Tokio interval that re-runs the KEM exchange
- Update the WireGuard config's PresharedKey and restart the tunnel service

#### [MODIFY] provider.go
- Server-side goroutine ticker that accepts re-negotiation requests
- Updates the WireGuard peer's PSK without removing/re-adding the peer

**Gate 4 test**: Tunnel stays up 15+ minutes, PSK changes at interval.

---

### Phase 6 — Tauri UI (Step 6 from runbook)

Surfshark-inspired design: dark theme, glassmorphic panels, animated connect button, real-time status.

#### [NEW] React frontend components:
- **ConnectButton**: Large animated circle button — pulse animation when connecting, solid green when connected, red when disconnected
- **StatusBadge**: "PQC Active" (green glow), "PQC Off" (amber), "Degraded" (red pulse)
- **IntegrityMonitor**: Expandable panel showing:
  - Last handshake timestamp
  - PQC key rotation countdown
  - Bytes in/out (animated counters)
  - Packet loss %
  - Tunnel uptime
- **ModeSelector**: Tabbed selector — Server / Peer Relay / Gaming (Peer + Gaming grayed out for v1 with "Coming Soon")
- **Settings**: Server address, auto-reconnect, kill switch toggle, DNS config

#### [NEW] Tauri IPC commands (commands.rs):
- `connect(mode: &str)` → runs PSK negotiation + tunnel_up
- `disconnect()` → tunnel_down
- `get_status()` → returns connection state, PQC status, stats
- `get_integrity()` → returns integrity monitor data

#### Design system (index.css):
- Dark background: `hsl(220, 20%, 8%)`
- Glass panels: `backdrop-filter: blur(20px)`, semi-transparent borders
- Accent: cyan-to-purple gradient for the connect ring
- Typography: Inter font family
- Micro-animations: smooth state transitions, breathing effects on active connection

---

### Phase 7 — Hardening (Step 7, stretch goal)

#### Kill switch
- Windows Filtering Platform (WFP) rules via `netsh advfirewall` to block all non-tunnel egress
- Persist kill switch state in Windows registry
- Automatically activate on connect, deactivate on clean disconnect

#### DNS leak protection
- NRPT (Name Resolution Policy Table) rules to force all DNS through tunnel
- Fallback: configure DoH to 1.1.1.1

#### Auto-reconnect
- Exponential backoff: 1s, 2s, 4s, 8s, max 30s
- Re-run PSK negotiation on reconnect

#### Key-at-rest protection
- DPAPI encryption for stored WireGuard private key

---

## Verification Plan

### Automated Tests
1. **Rust unit test**: `psk_agreement_roundtrip` (already in your code) — verifies ML-KEM generate → encapsulate → decapsulate → HKDF produces identical PSK
2. **Go unit test**: Same roundtrip test server-side
3. **Cross-language test**: Script that runs Rust encaps + Go decaps (or vice versa) and compares PSK — proves interop
4. **`cargo build`** and **`go build`** pass clean

### Manual Verification (Gate Tests)
- **Gate 2**: Rust binary controls WireGuard tunnel up/down, `wg show` confirms peer on server
- **Gate 3**: Both sides log identical PSK prefix, tunnel works with PQ-derived PSK
- **Gate 4**: 15-minute uptime test with rotating PSK
- **Gate 5**: HTTP API responds correctly to curl tests
- **Gate 6**: Tauri UI launches, connect button triggers full flow
