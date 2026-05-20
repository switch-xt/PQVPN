# PQVPN — IMPLEMENTATION PLAN FOR ANTIGRAVITY

**Status:** v2.0 — supersedes everything before it
**Audience:** Claude inside Antigravity, executing tasks in order
**Operator:** you, pasting prompts and verifying gates

---

## 0. HOW TO USE THIS DOCUMENT

Each task below is one Antigravity session. The format is:

- **Goal** — one sentence
- **Touches** — exact files
- **Context to load** — what Antigravity reads first
- **Prompt** — the literal text you paste
- **Gate** — what you run to prove it works before moving on

**Do not skip gates.** A failed gate two phases later costs a day; a failed gate at the gate itself costs five minutes. The whole document is sequenced so each gate is cheap to test and localizes failure.

---

## 1. CURRENT STATE (verified, not theoretical)

You have:

- Tauri + React + Rust client that builds and launches on Windows 10.
- WireGuard tunnel installs via `wireguard.exe /installtunnelservice` from elevated PowerShell — works.
- A UDP-over-TCP proxy on the client (`tunnel.rs`) and Python mirror on the server (`tcp_to_udp.py`).
- GCP VM at `34.136.62.117`, WireGuard `wg0` on UDP 51820 internally, TCP 443 exposed to the public via the Python proxy.
- Confirmed end-to-end: client IP shifted from Chennai → Mumbai. Tunnel handshake + at least one HTTP roundtrip works.
- Known fragility: single TCP connection without reconnect; MTU not set; PQ negotiation code exists but unverified that it actually runs; race between proxy spawn and `tunnel_up`; no DNS-leak verification.

You do **not** have, despite some UI claims to the contrary:

- Real PSK rotation.
- Real bytes-in/out (UI shows placeholder zeros).
- A kill switch.
- An installer.
- Auth on the server beyond placeholder JWT string.
- Mode 2 (peer relay) or Mode 3 (gaming) — UI tabs exist; backends do not.

---

## 2. DECISIONS LOCKED (do not re-debate, do not let Antigravity re-debate)

These are settled. If Antigravity proposes changing any of them, reject:

1. **Crypto**: `ml-kem` crate (pure Rust, FIPS 203) on client, Go 1.24 `crypto/mlkem` on server. **No liboqs anywhere.**
2. **Tunnel**: WireGuard NT via official `wireguard.exe` service. **No `wg-quick` on Windows** — it does not exist there.
3. **DPI evasion**: UDP-over-TCP-over-TLS on port 443. Raw TCP is *not* enough — the college DPI will fingerprint it.
4. **TLS trust**: pinned self-signed cert (or pinned Let's Encrypt cert) — **never system root CAs** for the control plane. We own both ends.
5. **Server language**: Python proxy is temporary. Final server is **Go 1.24+** for everything (TLS termination, KEM, peer registry, `wg set`).
6. **Mode 2 is deferred.** Not in scope until Phases 1–5 ship. NAT traversal is its own product.
7. **No DoH bypass for the client.** If Chrome's DoH leaks DNS, we configure Windows to override it via NRPT — we do not chase Chrome's behavior.

---

## 3. NORTH STAR (one sentence)

A Windows desktop client that, with one click, establishes a WireGuard tunnel to a GCP server through a TLS-wrapped TCP-on-443 transport, using an ML-KEM-768-derived rotating PSK, and survives hostile DPI for hours without manual intervention.

Everything in this plan exists to make that sentence true. Anything that doesn't serve it gets cut.

---

## 4. PHASE 1 — STABILIZE MODE 1 (urgent, 1–2 sessions)

**Phase goal:** Mode 1 holds a tunnel for ≥30 minutes on the college Wi-Fi with browsing working, no manual intervention. PQ negotiation provably runs.

### Task 1.1 — Apply the resilient proxy and MTU fix

**Goal:** Replace fragile `tunnel.rs`, add MTU=1280, eliminate the spawn/tunnel_up race.

**Touches:** `client/src-tauri/src/tunnel.rs`, `client/src-tauri/src/commands.rs`, `client/src-tauri/src/wireguard/windows.rs`.

**Context to load:** all three files above plus the resilient `tunnel.rs` and edits I provided in the prior conversation.

**Prompt for Antigravity:**
```
Apply three changes:

1. Replace tunnel.rs entirely with the resilient supervisor version
   provided. Key properties: bind UDP before returning, reconnect TCP with
   exponential backoff capped at 5s, three select! tasks per session,
   bounded mpsc channel.

2. In commands.rs::connect, replace the fire-and-forget
   tauri::async_runtime::spawn block with a tauri::async_runtime::block_on
   that awaits start_proxy and propagates errors via .map_err. The proxy
   must be ready BEFORE tunnel_up runs.

3. In wireguard/windows.rs::render_conf, add a line "MTU = 1280" inside
   the [Interface] block, immediately after the DNS line. Also add `pub mtu:
   u32` to WgConfParams with default 1280, and wire commands.rs to pass
   1280 explicitly.

Do not change anything else. Do not "improve" the proxy beyond these
edits. Do not touch pqc/. Do not touch the React frontend.
```

**Gate:**
- `cargo build` clean.
- Connect, browse for 10 minutes. Run `wg show wg0` on the server every 30 seconds (`watch -n 2 wg show wg0`); `transfer` numbers must monotonically increase the entire time.
- Kill the Python proxy on the server for ~10s, then restart it. Client should reconnect on its own within 30s without you touching the UI.

---

### Task 1.2 — Add real TLS to the transport (kills DPI fingerprinting)

**Goal:** The college DPI must see what looks like a real HTTPS connection, not raw bytes with a 2-byte length prefix on port 443.

**Touches:** `client/src-tauri/src/tunnel.rs`, `client/src-tauri/Cargo.toml`, `server/tcp_to_udp.py` (becomes `tls_to_udp.py`), GCP server setup.

**Context to load:** current `tunnel.rs`, current `tcp_to_udp.py`.

**Prompt for Antigravity:**
```
Convert the UDP-over-TCP proxy to UDP-over-TLS:

CLIENT (Rust):
- Add `tokio-rustls = "0.26"` and `rustls = "0.23"` to Cargo.toml. Also
  add `webpki-roots = "0.26"` only as a fallback path; primary path uses
  a pinned cert.
- Embed the server's certificate DER bytes via `include_bytes!` from a
  path like `client/src-tauri/resources/server_cert.der`. Add the file
  with a placeholder for now (we'll fill it in Task 1.3).
- In tunnel.rs supervisor(), replace `TcpStream::connect` with:
    1) TcpStream::connect (with set_nodelay(true))
    2) Wrap with tokio_rustls::TlsConnector configured with a
       RootCertStore containing ONLY the pinned cert.
    3) Use the resulting TlsStream the same way the old TcpStream was used.
- Server name for SNI: hardcode "pqvpn.local" (must match cert CN).

SERVER (Python, replace tcp_to_udp.py with tls_to_udp.py):
- Use asyncio ssl.SSLContext loaded with the server cert + key.
- ssl_context.minimum_version = TLSVersion.TLSv1_3
- Wrap the accept loop with the SSL context. Everything else stays
  identical — same 2-byte length prefix, same WG forwarding.
- Cert + key path: /etc/pqvpn/server.crt, /etc/pqvpn/server.key

NOTES:
- This is real TLS — the wire actually looks like HTTPS to DPI.
- The client trusts ONLY the pinned cert. Do not fall back to
  webpki-roots in this path; that would defeat pinning.
- The cert in Task 1.3 will be regenerated; this task just wires the
  plumbing.
```

**Gate:** Skip running until Task 1.3 generates the cert. Code should compile though.

---

### Task 1.3 — Generate and pin the server certificate

**Goal:** A self-signed TLS cert whose fingerprint is compiled into the client. We own both ends, so public-CA trust is irrelevant.

**Touches:** GCP server (manual openssl), `client/src-tauri/resources/server_cert.der`, `client/src-tauri/build.rs` (new).

**Manual on server:**
```bash
sudo mkdir -p /etc/pqvpn
sudo openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout /etc/pqvpn/server.key \
  -out /etc/pqvpn/server.crt \
  -days 3650 -subj "/CN=pqvpn.local"
sudo openssl x509 -in /etc/pqvpn/server.crt -outform DER \
  -out /etc/pqvpn/server.der
sudo chmod 600 /etc/pqvpn/server.key
```
SCP `server.der` to `client/src-tauri/resources/server_cert.der`.

**Prompt for Antigravity:**
```
Verify that:
1. client/src-tauri/resources/server_cert.der exists.
2. tunnel.rs loads it via include_bytes!("../resources/server_cert.der").
3. The TlsConnector's RootCertStore has only this cert added.
4. If the file is missing at build time, compilation fails with a clear
   error (not a warning).

No code changes if all four hold; otherwise wire them up.
```

**Gate:**
- Restart Python proxy as `tls_to_udp.py` on server.
- Client connects via UI.
- `tcpdump -i ens4 -A -s0 'tcp port 443'` on the server shows TLS ClientHello / ServerHello bytes — not raw garbage. After the handshake the payload is unreadable (because it's TLS-encrypted), which is exactly the point.
- Tunnel up, ping `10.8.0.1`, browse. Same stability as Task 1.1.

---

### Task 1.4 — Prove PQ negotiation actually runs

**Goal:** Right now `commands.rs::connect` silently falls back to a hardcoded server pubkey if `pqc::negotiate` fails. The PQC path may be dead and the UI would never tell you. Make it provable.

**Touches:** `client/src-tauri/src/commands.rs`, `client/src-tauri/src/pqc/mod.rs`, server (whichever process answers the KEM endpoint).

**Prompt for Antigravity:**
```
Audit and harden the PQ negotiation path:

1. In commands.rs::connect, the current code matches Ok/Err on
   pqc::negotiate and silently falls back on Err with a comment "PQ API
   unreachable". Replace this fallback with a hard fail:
     - If config.pq_required is true (default true): return Err with the
       precise underlying error, including server host:port attempted.
     - If pq_required is false (only for an explicit gaming mode toggle):
       log a loud WARNING and proceed without PSK.

2. Add a connect-time log line on success that prints:
     "[pqc] negotiated PSK with server, b64 prefix: <first 8 chars>..."
   This prefix is the proof in logs that PQ actually ran.

3. Expose the PSK base64 prefix in get_status() as
   psk_fingerprint: Option<String> (first 8 chars only, never the full
   PSK). Frontend can display this for visible confirmation.

4. Verify that the server side actually has a KEM responder running on
   server_port. If it does not, this task includes adding a minimal Go
   responder (see Task 3.1 in the master plan) — but for THIS task, if
   no server-side endpoint exists yet, document it in
   docs/PHASE1_KNOWN_GAPS.md and set pq_required to false by default in
   AppConfig until Phase 3 ships the real server.
```

**Gate:**
- Reading logs during connect, you see the `[pqc] negotiated PSK...` line. If you don't, PQ is not running and Phase 1 is not complete.
- UI shows the 8-char PSK fingerprint somewhere visible.
- Reconnecting produces a *different* fingerprint each time (proves ephemeral keys, not cached).

---

### Task 1.5 — DNS leak verification + fix if leaking

**Goal:** Confirm DNS for browsing goes through the tunnel. If not, fix it via Windows NRPT.

**Touches:** `client/src-tauri/src/dns/windows.rs` (new), `client/src-tauri/src/commands.rs`.

**Diagnostic first (manual, no code):**
```powershell
# While connected:
Resolve-DnsClient | Select-Object Name, NameServer, InterfaceAlias
nslookup youtube.com
nslookup youtube.com 1.1.1.1
```
If the answers differ, DNS is leaking. If the first nslookup returns NXDOMAIN or wrong IP, college DNS is being used despite the tunnel.

**Prompt for Antigravity (only if leaking):**
```
Add a Windows-only module client/src-tauri/src/dns/windows.rs that, on
tunnel_up, executes via elevated PowerShell:

  Add-DnsClientNrptRule -Namespace "." \
    -NameServers "1.1.1.1","1.0.0.1" \
    -DisplayName "PQVPN" \
    -Comment "PQVPN forced DNS"

And on tunnel_down:

  Get-DnsClientNrptRule | Where-Object DisplayName -eq "PQVPN" | \
    Remove-DnsClientNrptRule -Force

Wire these into commands.rs::connect (after tunnel_up succeeds) and
commands.rs::disconnect (before tunnel_down). On any error from the
NRPT calls, log loudly but do NOT fail the connection — DNS leak is a
silent-failure problem, not a connect-failure problem.

Also: in wireguard/windows.rs::render_conf, ensure DNS = 1.1.1.1, 1.0.0.1
is set unconditionally.
```

**Gate:** After fix, `nslookup youtube.com` and `nslookup youtube.com 1.1.1.1` return identical answers, and a browser hitting the blocked site loads.

---

### Phase 1 EXIT CRITERIA

All four must be true before touching Phase 2:

1. Tunnel survives 30 minutes of normal browsing without UI restart.
2. `[pqc] negotiated PSK` appears in logs every connect; fingerprint visible in UI.
3. `tcpdump` on server shows TLS, not raw bytes, on port 443.
4. Blocked sites load.

If any of the four fail, stop and debug *that one*. Do not work around it by moving on.

---

## 5. PHASE 2 — PRODUCTION HARDENING

**Phase goal:** The product survives real-world misuse: tunnel deaths, sleep/wake, crashes, hostile networks that drop your TCP every 60s.

### Task 2.1 — PSK rotation loop

**Goal:** Re-run the ML-KEM negotiation every 90 seconds and update the WireGuard PSK on both ends, so the post-quantum claim is not a one-time thing.

**Touches:** `client/src-tauri/src/pqc/rotation.rs` (new), `commands.rs`, server KEM responder (Go, Phase 3) or interim Python.

**Prompt for Antigravity:**
```
Implement client-side PSK rotation:

1. New file client/src-tauri/src/pqc/rotation.rs. On connect, spawn a
   tokio task that:
     a. Sleeps for 90 seconds.
     b. Calls pqc::negotiate() again — ephemeral keys, fresh PSK.
     c. Updates the WireGuard interface's preshared-key without
        recreating the tunnel. On Windows this means writing the new
        config and calling wireguard.exe /syncconf — verify this command
        works; if not, fall back to /uninstalltunnelservice +
        /installtunnelservice (1-2 sec blip but acceptable).
     d. Loops.

2. The rotation task must be cancelled on disconnect (use a
   tokio_util::sync::CancellationToken stored in AppState).

3. Add metric to get_status(): psk_age_secs already exists — make it
   actually reflect time since last rotation, not just initial connect.

4. Server side: each rotation is a NEW ML-KEM exchange. Server must
   wg set the new PSK on the peer entry. Document this requirement
   for Phase 3 implementation; for now, if server doesn't support
   mid-session rotation, log a warning and skip rotation (don't tear
   tunnel down).

UNCERTAINTY FLAG: wireguard.exe /syncconf behavior under PSK change is
worth verifying empirically before depending on it. If it doesn't
propagate PSK changes, we use the reinstall path.
```

**Gate:** After connect, every 90 seconds, the PSK fingerprint shown in UI changes, and the tunnel stays up (no drop). `wg show wg0 preshared-keys` on server reflects the new PSK each rotation.

---

### Task 2.2 — Windows kill switch via WFP

**Goal:** When tunnel drops, all internet traffic blocks until tunnel returns. No silent leak.

**Touches:** `client/src-tauri/src/killswitch/windows.rs` (new), `commands.rs`, `Cargo.toml`.

**Prompt for Antigravity:**
```
Implement a kill switch using Windows Filtering Platform:

1. Add to Cargo.toml: `windows = { version = "0.58", features = [
   "Win32_NetworkManagement_WindowsFilteringPlatform",
   "Win32_Foundation",
   "Win32_System_Rpc",
] }`.

2. Create client/src-tauri/src/killswitch/windows.rs with:
     - fn enable(tunnel_interface_luid: u64) -> Result<()>
         Adds WFP filters that BLOCK all outbound traffic at
         FWPM_LAYER_ALE_AUTH_CONNECT_V4/V6 EXCEPT:
           a) loopback
           b) traffic on the tunnel interface LUID
           c) outbound to the server endpoint IP:443 (so the proxy
              can reconnect TLS without being killswitch'd)
     - fn disable() -> Result<()>
         Removes the filters by GUID stored in a static OnceLock.

3. Expose Tauri command toggle_kill_switch(enabled: bool).

4. Persist kill switch state in a small JSON file under
   %LOCALAPPDATA%\PQVPN\state.json so it survives crashes. On app
   startup, if state says enabled but no tunnel exists, re-apply the
   filters before doing anything else.

5. Add a tray-menu item "Emergency disable kill switch" that calls
   disable() and writes false to state.json. This is the user's escape
   hatch if the app itself crashes.

CRITICAL CORRECTNESS:
- The tunnel interface LUID is obtained via GetIfTable2 or
  ConvertInterfaceAliasToLuid AFTER wireguard.exe creates the
  interface. tunnel_up() must return the LUID; enable() takes it as
  argument. Do not hardcode interface names.
- The server endpoint IP exception is critical: without it, the killswitch
  blocks the very TLS connection the proxy needs to re-establish.
```

**Gate:**
- Enable kill switch in UI.
- Connect — internet works.
- Kill the Python proxy on server. Within 30s, client's TCP fails.
- Internet on Windows stops working entirely (curl any external site → connection refused / timeout). Localhost still works.
- Restart Python proxy. Client reconnects. Internet returns.
- Disable kill switch. Disconnect. Internet works normally again.

---

### Task 2.3 — Real WireGuard interface stats

**Goal:** Get actual bytes-in/out, latest handshake, peer state for the integrity monitor. No more zeros.

**Touches:** `client/src-tauri/src/wireguard/stats.rs` (new), `commands.rs::get_status`.

**Prompt for Antigravity:**
```
Add real WireGuard interface stats on Windows:

1. New file client/src-tauri/src/wireguard/stats.rs.

2. Implementation approach: invoke wireguard.exe /dumpinterface <name>
   (verify this exists; if not, parse the output of `wg.exe show
   <name> dump` shipped alongside wireguard.exe). Returns
   tab-separated fields including:
     latest_handshake (unix timestamp)
     transfer_rx (bytes)
     transfer_tx (bytes)

3. Parse into:
     pub struct InterfaceStats {
         pub bytes_in: u64,
         pub bytes_out: u64,
         pub latest_handshake_secs_ago: Option<u64>,
     }

4. In commands.rs::get_status(), call stats::read("pqvpn0") and fill
   the VpnStatus fields. Update VpnStatus to include
   latest_handshake_secs_ago.

5. Frontend (App.tsx): wire latest_handshake into the integrity panel
   already present. Color: green <30s, yellow 30-120s, red >120s.

UNCERTAINTY FLAG: confirm the exact wg.exe binary path bundled with
WireGuard for Windows. If wg.exe is not present, an alternative is the
WireGuard NT IOCTL via the windows crate's
Win32_NetworkManagement_WireGuard — more complex but doesn't shell out.
```

**Gate:** UI's bytes-in/out increase in real time during browsing. Latest handshake counts up from 0 to ~120s and then resets when WG rekeys.

---

### Task 2.4 — Auto-reconnect at the app level

**Goal:** If the tunnel drops *and* the proxy supervisor can't restore it (e.g. machine sleeps and Wi-Fi changes networks on wake), the whole connect flow re-runs without user intervention.

**Touches:** `client/src-tauri/src/health.rs` (new), `commands.rs`.

**Prompt for Antigravity:**
```
Add a health watchdog:

1. New file client/src-tauri/src/health.rs. Tokio task spawned on
   connect, cancelled on disconnect. Every 10s:
     a. Call wireguard::tunnel_is_up(TUNNEL_NAME). If false → reconnect.
     b. Read interface stats; if latest_handshake > 180s ago → reconnect.
     c. Listen for Windows network change events
        (NotifyAddrChange / NotifyRouteChange via the windows crate);
        on any event, force a reconnect.

2. "Reconnect" means: cancel rotation task, tunnel_down, fresh
   negotiate(), tunnel_up. Same flow as user-initiated connect.

3. Use a backoff: 1s, 2s, 4s, 8s, 16s, then 30s steady. Cap retries at
   "forever" — VPN apps should never give up; the user disconnects
   when they want to.

4. Emit a Tauri event on every state transition so the UI can show
   "reconnecting...".

5. Frontend: listen for tunnel_state_changed event in App.tsx and
   update the connect button state accordingly.
```

**Gate:** Put laptop to sleep for 2 minutes. Wake. Within 30 seconds the tunnel is back up without you clicking anything. Switch Wi-Fi networks (mobile hotspot → college Wi-Fi). Tunnel re-establishes.

---

### Phase 2 EXIT CRITERIA

1. PSK rotates every 90s; tunnel stays up across rotations.
2. Kill switch blocks all leaks when tunnel drops.
3. UI shows real bytes and handshake age.
4. Sleep/wake/network-change → automatic reconnect with no user action.

---

## 6. PHASE 3 — REAL SERVER (replace Python with Go)

**Phase goal:** Production-grade server: Go daemon, real auth, peer registry, TLS termination native, ML-KEM responder built in. Python proxy retired.

### Task 3.1 — Go daemon skeleton + TLS-terminating proxy

**Goal:** Single Go binary `pqvpnd` that replaces `tls_to_udp.py`. Same wire protocol.

**Touches:** `server/cmd/pqvpnd/main.go`, `server/internal/proxy/proxy.go`, `server/go.mod`.

**Prompt for Antigravity:**
```
Create a Go 1.24 daemon that replaces tls_to_udp.py:

1. go.mod with go 1.24. No third-party deps beyond stdlib for now.

2. server/internal/proxy/proxy.go:
     - Serve(tlsCfg *tls.Config, listenAddr, wgUDPAddr string) error
     - For each accepted TLS conn, spawn a goroutine that bidir-pumps:
         tcp→udp: read 2-byte LE length, read N bytes, send to wgUDPAddr.
         udp→tcp: read from a per-conn UDP socket bound to ephemeral
                  port + connect()'d to wgUDPAddr, frame, write.
     - Match the existing Python wire format byte-for-byte.

3. server/cmd/pqvpnd/main.go:
     - Loads /etc/pqvpn/server.crt + /etc/pqvpn/server.key.
     - tls.Config: MinVersion = TLS13, single cert.
     - Calls proxy.Serve(...).

4. Build:
     cd server && go build -o /tmp/pqvpnd ./cmd/pqvpnd
     SCP /tmp/pqvpnd to GCP server /usr/local/bin/pqvpnd

5. Systemd unit infra/systemd/pqvpnd.service:
     [Unit]
     Description=PQVPN daemon
     After=network.target wg-quick@wg0.service
     [Service]
     ExecStart=/usr/local/bin/pqvpnd
     Restart=always
     RestartSec=2
     [Install]
     WantedBy=multi-user.target

   Document install: systemctl enable --now pqvpnd, systemctl disable
   tls-to-udp (or whatever the Python systemd unit was named).
```

**Gate:** Stop Python proxy on server, start Go daemon. Client connects identically — same behavior, same logs, same tcpdump output. If anything differs, Go daemon has a bug.

---

### Task 3.2 — KEM responder in Go

**Goal:** Server-side ML-KEM-768 exchange endpoint. Replaces whatever placeholder responded to `pqc::negotiate`.

**Touches:** `server/internal/kem/responder.go`, `server/cmd/pqvpnd/main.go`.

**Prompt for Antigravity:**
```
Add ML-KEM-768 negotiation endpoint:

1. server/internal/kem/responder.go:
     - HandleConn(conn net.Conn, cfg Config, auth AuthFunc) error
     - Wire format must match client/src-tauri/src/pqc/psk_provider.rs
       EXACTLY:
         in:  newline-delimited JSON ClientHello {ek_b64, wg_pubkey,
              auth_token}
         out: newline-delimited JSON ServerResponse {ct_b64,
              server_wg_pubkey, server_endpoint, hkdf_salt_b64}
     - Use crypto/mlkem (Go 1.24 stdlib). DO NOT import liboqs-go.
     - HKDF-SHA256 with info "pqvpn-psk-v1", 32-byte output.
     - On success, run `wg set wg0 peer <wg_pubkey> preshared-key
       /dev/stdin allowed-ips 10.8.0.<n>/32` where /dev/stdin receives
       the base64 PSK (PSK never appears on argv).

2. server/internal/registry/peers.go: SQLite peer registry.
   Schema:
     peers(id TEXT PK, wg_pubkey TEXT UNIQUE, assigned_ip TEXT UNIQUE,
           first_seen INT, last_seen INT)
   The KEM handler calls registry.GetOrAssign(wg_pubkey) to determine
   the AllowedIP. Use github.com/mattn/go-sqlite3 (CGO-free build is
   fine for our scale via modernc.org/sqlite if preferred).

3. cmd/pqvpnd/main.go: Bind a SECOND listener on TCP 8443 (TLS, same
   pinned cert) for the KEM endpoint. Port 443 stays the WireGuard
   proxy. Client config: pq_endpoint_port = 8443.

4. AuthFunc: for now, accept any auth_token. JWT verification is
   Task 3.3.

CRITICAL: PSK rotation (Task 2.1) means this endpoint must be
re-callable mid-session. Each call updates the WG peer's preshared-key
via `wg set`. No teardown of the WG peer between rotations.
```

**Gate:**
- Restart pqvpnd. Client connects.
- Server logs show: `KEM negotiated, psk prefix <8 chars>` matching what client logs.
- `wg show wg0 preshared-keys` shows the negotiated PSK for the client peer.
- After 90s, both sides log a rotation with a different PSK prefix.

---

### Task 3.3 — JWT auth + peer registration

**Goal:** Real auth. No more `auth-token-placeholder`.

**Touches:** `server/internal/auth/jwt.go`, registration flow, client `commands.rs`.

**Prompt for Antigravity:**
```
Implement JWT-based peer authentication:

1. server/internal/auth/jwt.go:
     - HS256 with a server-side secret read from /etc/pqvpn/jwt.secret
       (random 32 bytes generated at install time).
     - Token payload: { peer_id, exp }
     - Issue() and Verify() functions.

2. New endpoint on TCP 8443 (TLS): POST /register
     - Body: { wg_pubkey, identity_hint } (identity_hint = e.g. machine
       name, advisory only)
     - Creates peer in registry, returns JWT.
     - Rate limit: 5 per IP per hour.

3. Client (commands.rs): on first connect (no JWT cached), call
   /register first, save JWT to %LOCALAPPDATA%\PQVPN\state.json.
   Subsequent connects: send JWT in ClientHello.auth_token.

4. Server KEM handler: AuthFunc verifies JWT, rejects if invalid/expired.

5. Encrypt the cached JWT at rest using Windows DPAPI (via the windows
   crate's Win32_Security_Cryptography or the windows-dpapi crate).
   Never write the JWT to disk plaintext.
```

**Gate:** Delete `state.json`, connect → registers + gets JWT, connects. Connect again → uses cached JWT, no register call. Manually corrupt the JWT in state.json → connect fails with clear auth error.

---

### Phase 3 EXIT CRITERIA

1. Python proxy decommissioned; Go daemon is the only server process.
2. JWT auth works; placeholder string is gone.
3. SQLite peer registry assigns and persists tunnel IPs.
4. PSK rotation works end-to-end with Go server.

---

## 7. PHASE 4 — UI COMPLETION + GAMING MODE

**Phase goal:** UI no longer has placeholder values. Gaming mode (Mode 3) works for non-restrictive networks.

### Task 4.1 — Wire integrity monitor to real data

**Goal:** Every number in the integrity panel comes from real backend data. No frontend mocks.

**Touches:** `client/src/App.tsx`, `client/src/components/*`, `commands.rs::get_status`.

**Prompt for Antigravity:**
```
Audit App.tsx for any UI value that is hardcoded, randomized, or
generated client-side, and replace it with a value from get_status().
Specifically:

- bytes_in, bytes_out → real (Task 2.3)
- psk_age_secs → real (Task 2.1)
- latest_handshake → add to VpnStatus (Task 2.3) and surface
- psk_fingerprint → first 8 chars of PSK base64 (Task 1.4)
- Server WG pubkey display ("ivYdRx...pKhk8=") → read from
  connection state, not hardcoded in App.tsx
- "Latency" if displayed → measure with a real ping (UDP echo to
  10.8.0.1 via the tunnel; new Rust command)
- Platform/Browser/Screen → these are fine to leave detected
  client-side, but Proxy ("VPN Active") must reflect actual tunnel state

If anything in App.tsx is faked, delete the fake and call get_status
for it instead. If get_status doesn't have it, add the field.
```

**Gate:** Inspect with React DevTools: no `useState` initialized to fake-looking constants (e.g. "1.2 MB", "42ms") that don't update.

---

### Task 4.2 — Gaming Mode (Mode 3)

**Goal:** A toggle that disables the TLS-wrapped TCP proxy and connects directly via UDP 51820. For networks without DPI. Lower latency, lower CPU.

**Touches:** `commands.rs`, `wireguard/windows.rs`, `App.tsx`.

**Prompt for Antigravity:**
```
Implement Gaming Mode:

1. Add Mode enum to commands.rs:
     pub enum ConnectionMode { Server, Gaming }
   (Peer is deferred — do not add it.)

2. connect() takes mode: ConnectionMode parameter. Frontend's
   ModeSelector passes it.

3. In Gaming mode:
     - DO start the PQ negotiation (Phase 2's rotation still applies —
       the *post-quantum* part remains, only the transport changes).
     - DO NOT start the TLS-over-TCP proxy.
     - WireGuard endpoint in the conf = real server IP:51820, not
       127.0.0.1.
     - render_conf in this mode uses normal UDP endpoint.

4. UI: When user selects Gaming, show a tooltip:
     "Gaming Mode uses direct UDP. Faster, but may not work on
      networks that block VPN protocols."
   No "PQ disabled" warning — PQ is still on. The original spec was
   wrong about this; PQ is cheap and stays.

5. Mode switching while connected: disconnect, switch, reconnect.
   No in-place mode change.
```

**Gate:** On home Wi-Fi (no DPI): Gaming mode connects in <2s, ping to 10.8.0.1 ~10-30ms lower than Server mode. On college Wi-Fi: Gaming mode fails to connect within 10s (UDP blocked); UI shows a clear error and suggests Server mode.

---

### Task 4.3 — Settings screen + diagnostics view

**Goal:** A second screen where the user can change DNS, MTU, server endpoint, view full logs, run connection diagnostics.

**Touches:** `client/src/screens/Settings.tsx` (new), `commands.rs::run_diagnostics`.

**Prompt for Antigravity:**
```
Add a Settings screen accessed via a gear icon in the header:

Tabs:
  General — Auto-connect on launch, Start with Windows
  Network — DNS servers (default 1.1.1.1), MTU (default 1280),
            Server endpoint host (default 34.136.62.117)
  Security — Kill switch toggle, "Require PQ" toggle (default on)
  Diagnostics — "Run diagnostics" button

run_diagnostics Rust command:
  - Ping 10.8.0.1 5 times, report avg/loss.
  - nslookup youtube.com via current DNS, report result.
  - Check tunnel_is_up.
  - Read interface stats.
  - Last 100 lines of in-memory log buffer.
  Returns a single struct, frontend renders as a copyable text block.

Persist settings to %LOCALAPPDATA%\PQVPN\settings.json. Load on app
startup. Changes apply on next connect (no hot reload).
```

**Gate:** Change MTU to 1200 in settings, reconnect, verify in the rendered conf file under %LOCALAPPDATA%\PQVPN\tunnels\pqvpn0.conf that MTU = 1200 is set.

---

## 8. PHASE 5 — INSTALLER + RELEASE

**Phase goal:** A signed `.exe` you can hand to anyone. Auto-updater built in.

### Task 5.1 — Bundle WireGuard as sidecar

**Goal:** No dependency on a pre-installed WireGuard for Windows.

**Touches:** `client/src-tauri/tauri.conf.json`, `client/src-tauri/binaries/`, `wireguard/windows.rs`.

**Prompt for Antigravity:**
```
Configure Tauri sidecar to bundle wireguard.exe:

1. Download official WireGuard for Windows MSI, extract
   wireguard.exe, wg.exe, and the wireguard-nt driver. Place in
   client/src-tauri/binaries/wireguard-x86_64-pc-windows-msvc.exe and
   wg-x86_64-pc-windows-msvc.exe.

2. Update tauri.conf.json:
     "tauri": {
       "bundle": {
         "externalBin": ["binaries/wireguard", "binaries/wg"]
       }
     }

3. Modify wireguard/windows.rs::wg_exe() to resolve to the sidecar via
   tauri::api::process::Command::new_sidecar("wireguard") path
   resolution instead of hardcoding C:\Program Files\WireGuard.

4. The wireguard-nt driver still needs installing as a service. Add a
   post-install step in the NSIS installer (Task 5.2) that runs
   wireguard.exe /installmanagerservice once.

UNCERTAINTY: bundling the WireGuard NT driver involves a signed .sys
file. If your build doesn't have a code signing cert yet, on first
launch ask the user to install official WireGuard once and skip
bundling the driver. wireguard.exe itself can still be bundled.
```

**Gate:** Uninstall WireGuard for Windows from the dev machine. App still works (after post-install / first-launch driver setup).

---

### Task 5.2 — NSIS installer + Tauri updater

**Goal:** A signed installer that auto-updates.

**Touches:** `client/src-tauri/tauri.conf.json`, `.github/workflows/release.yml`.

**Prompt for Antigravity:**
```
Configure Tauri's NSIS installer + the updater plugin:

1. Add `tauri-plugin-updater = "2"` to Cargo.toml and register the
   plugin in lib.rs.

2. tauri.conf.json:
     "bundle": {
       "windows": {
         "nsis": {
           "installerIcon": "icons/icon.ico",
           "installMode": "perMachine",
           "displayLanguageSelector": false
         }
       }
     },
     "plugins": {
       "updater": {
         "endpoints": ["https://<your-update-server>/latest.json"],
         "pubkey": "<generated via tauri signer generate>"
       }
     }

3. GitHub Action .github/workflows/release.yml that on tag v*:
     - Builds Tauri NSIS installer.
     - Signs the installer with sign tool (cert path from secrets).
     - Uploads installer + signature to GitHub Releases.
     - Generates latest.json pointing to the release assets and
       uploads to the update server.

CODE SIGNING: Without an EV code signing cert, the installer triggers
SmartScreen warnings. For an MVP this is acceptable; document that
users will see the warning and need to "Run anyway".
```

**Gate:** Run `npm run tauri build`. Resulting installer in `client/src-tauri/target/release/bundle/nsis/*.exe` runs on a clean Windows VM and produces a working app.

---

## 9. PHASE 6 — MODE 2 (PEER RELAY) — DEFERRED

Do not start until Phases 1–5 are shipped and stable. When you do:

- Do NOT hand-roll STUN. Use the `webrtc-rs` or `pion/stun` library directly. Symmetric NAT (~25% of consumer routers) will fail hole-punching; relay fallback through the GCP server is mandatory, not optional.
- Reuse the existing TLS + JWT auth for the signaling channel.
- Same PQ + PSK rotation as Mode 1; only the WG endpoint differs.
- This is the hardest mode; budget ~10x more time than the original doc claimed.

---

## 10. ANTI-PATTERNS — REJECT IF ANTIGRAVITY PROPOSES THEM

Things that look reasonable but will set the project back. Reject without debate:

- **"Let's add liboqs for FIPS compliance."** No. Pure-Rust `ml-kem` is FIPS 203. liboqs adds a C dependency we already engineered out.
- **"We should use `wg-quick` on Windows."** It does not exist on Windows. WireGuard for Windows uses tunnel services.
- **"Trust system root CAs as a fallback."** No. We pin our cert. System roots open MITM.
- **"Just use TCP without TLS to simplify."** Phase 1 proved DPI fingerprints raw TCP. TLS is non-negotiable.
- **"Add a fallback to connect without PQ if it fails."** That's exactly what made PQ silently dead. Hard-fail unless gaming mode explicitly opted in.
- **"Run wireguard.exe directly without elevation."** Will fail on `/installtunnelservice`. Elevation is required and unavoidable.
- **"Persist the PSK to disk for faster reconnect."** Defeats the entire "post-quantum" property (PSK becomes long-lived secret on disk). PSK lives in memory only.
- **"Let's use Electron."** 150MB binary. We already chose Tauri.
- **"Add peer relay in Phase 1 since it's just UDP hole-punching."** No. Read Phase 6.

---

## 11. WORKING WITH ANTIGRAVITY (operator handbook)

- **One task per session.** Don't give it Phase 1 wholesale. Give it Task 1.1, verify gate, then Task 1.2.
- **Always paste the "Context to load" files first**, then the prompt. Antigravity is much better when it can see the current state of the file it's editing.
- **After each task, run the gate yourself.** Do not trust "the code compiles" as evidence; the gate is what proves the task is done.
- **When a gate fails**, paste the failure output back to Antigravity and ask: "What's the simplest explanation?" Do not let it propose architectural changes — only debugging.
- **Commit after every passed gate.** Branch name = task ID (e.g. `task/1.3-pinned-cert`). Squash-merge to `develop`.
- **At end of every phase**, write a 5-line PHASE_N_RETROSPECTIVE.md noting what was unexpected and what the next phase should change. This is for *you*, not Antigravity.

---

## 12. PROJECTED ORDERING

If you work nonstop and gates pass first try:

- Phase 1 (stabilize): 4-6 hours
- Phase 2 (harden): 8-10 hours
- Phase 3 (Go server): 6-8 hours
- Phase 4 (UI + Gaming): 4-6 hours
- Phase 5 (installer): 4-6 hours

Realistic estimate with gate retries and debugging: **2-3x the above.** The bottleneck will be Phase 2's kill switch (WFP is finicky) and Phase 3's first end-to-end PSK rotation across the new Go server. Plan for both to take a full session each.

Phase 6 (peer relay), when you do it, is a project of its own — budget 3-5 days minimum.

---

## 13. NORTH STAR — FINAL CHECK

Before tagging v1.0 release, all of these must be demonstrably true:

1. Install the signed .exe on a fresh Windows VM.
2. Launch, click Connect.
3. Within 5 seconds: tunnel up, PQ negotiated (fingerprint visible), DNS through tunnel.
4. Browse a blocked site successfully.
5. Leave running for 1 hour while doing normal work. Zero manual interventions. PSK rotates ~40 times silently.
6. Pull network cable for 30 seconds, re-plug. Tunnel restores itself, kill switch held the line during outage.
7. Reboot the GCP server. Within 30 seconds of it coming back, client reconnects.

If any of those seven fail, you have not shipped v1.0. Don't fake it. The reason we built all of Phases 1-5 is to make those seven true. Falling short on any one means a hole in the build.

Ship when all seven pass. Not before.
