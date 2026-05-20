# PQVPN — RUNBOOK (tonight, nonstop)

Rule: do not start step N+1 until step N's GATE passes. The gates exist so a
failure shows up where it is cheap to fix, not three layers later.

Architecture locked: client ↔ TLS(pinned cert) ↔ server, ML-KEM-768 over that
channel produces a 32-byte PSK, WireGuard uses it as PresharedKey. Pure-Rust
`ml-kem` on client, Go 1.24 stdlib `crypto/mlkem` on server. No liboqs anywhere.

---

## STEP 1 — Plain WireGuard tunnel, zero code (~30 min)

This proves the entire networking foundation. 90% of a VPN is here.

Server (GCP, Ubuntu 22.04, static IP reserved, UDP 51820 open in firewall):
```
sudo apt update && sudo apt install -y wireguard
wg genkey | tee server.key | wg pubkey > server.pub
# /etc/wireguard/wg0.conf:
#   [Interface]
#   Address = 10.8.0.1/24
#   ListenPort = 51820
#   PrivateKey = <server.key>
#   PostUp   = iptables -t nat -A POSTROUTING -o ens4 -j MASQUERADE; sysctl -w net.ipv4.ip_forward=1
#   PostDown = iptables -t nat -D POSTROUTING -o ens4 -j MASQUERADE
#   [Peer]
#   PublicKey = <client.pub>
#   AllowedIPs = 10.8.0.2/32
sudo wg-quick up wg0
```
(Replace `ens4` with the server's real WAN interface from `ip route`.)

Client (Windows, official WireGuard app):
```
[Interface]
PrivateKey = <client.key>
Address = 10.8.0.2/24
DNS = 1.1.1.1
[Peer]
PublicKey = <server.pub>
Endpoint = <SERVER_STATIC_IP>:51820
AllowedIPs = 0.0.0.0/0
PersistentKeepalive = 25
```

**GATE 1:** With the tunnel active, a browser shows the GCP server's IP, and
`ping 1.1.1.1` works. If not: it's NAT/forwarding/firewall on the server or
AllowedIPs on the client. Fix here. Nothing post-quantum matters until this works.

---

## STEP 2 — Rust controls the tunnel (no crypto)

Client writes a `.conf` and drives the Windows tunnel service. Code:
`client/src-tauri/src/wireguard/windows.rs` (delivered).

**GATE 2:** A Rust binary brings the *same* tunnel from Step 1 up and down.
`wg show` (server) shows the peer handshaking when up, gone when down.

---

## STEP 3 — The PSK provider (this is the post-quantum moment)

Flow per connection:
1. Client generates ephemeral ML-KEM-768 keypair.
2. Client → server over TLS (server cert pinned): ML-KEM encaps-key bytes + its
   WireGuard public key + identity (JWT or ML-DSA sig — JWT is fine for v1).
3. Server: `ek.Encapsulate()` → (shared_secret, ciphertext). Returns ciphertext.
4. Server: `psk = HKDF-SHA256(shared_secret, salt, "pqvpn-psk-v1", 32)`.
   Server runs `wg set wg0 peer <client_wg_pub> preshared-key <psk> allowed-ips 10.8.0.2/32`.
5. Client: `dk.Decapsulate(ciphertext)` → same shared_secret → same HKDF → same PSK.
6. Client writes the WireGuard conf with `PresharedKey = <psk>` and brings the
   tunnel up (Step 2 machinery).

Files delivered: `client/.../pqc/psk_provider.rs`, `server/.../psk/provider.go`.

**GATE 3:** Both sides log an identical base64 PSK. `wg show wg0 preshared-keys`
on the server shows it set (not "none"). Tunnel handshakes and `ping` works
**through a PSK that was negotiated post-quantum.** You now have a real PQ VPN.

---

## STEP 4 — PSK rotation

WireGuard rekeys ~every 2 min. Re-run Step 3's exchange on a timer (e.g. every
100s), update both ends' PSK before the window closes. Tokio interval on client,
goroutine ticker on server.

**GATE 4:** Tunnel stays up and pings continuously for 15+ minutes; logs show
the PSK changing at the interval.

---

## STEP 5 — Server daemon proper

Wrap Step 3's server side in: POST /register (store wg_pub), POST /connect
(the KEM exchange + wg peer add), DELETE /disconnect (wg peer remove), GET
/status. SQLite peer table. Systemd unit. Self-signed cert; client pins it.

## STEP 6 — Tauri UI
Connect button → `invoke('connect')` → runs Step 3. Status poll → integrity
monitor. Pure presentation; the engine already works.

## STEP 7 — Hardening (makes it not a toy)
Windows WFP kill switch (block all non-tunnel egress, persist to registry),
NRPT DNS-leak block, exponential-backoff auto-reconnect, DPAPI key-at-rest.

---

## UNCERTAINTIES — flagged honestly

- `ml-kem` crate API names have churned across versions. The delivered Rust uses
  the current shape; verify the 3 calls (`generate`, `encapsulate`,
  `decapsulate`) against `cargo doc -p ml-kem --open` and adjust if a method
  renamed. Logic is correct; only symbol names are at risk.
- Go `crypto/mlkem` requires Go **1.24+**. On 1.22/1.23 it does not exist —
  upgrade the server's Go, do not fall back to liboqs-go.
- Pinned-cert TLS is correct ONLY because you control both endpoints. Ship the
  server cert fingerprint compiled into the client. Do not use public-CA trust.
- Server WAN interface name (`ens4` above) varies per GCP image. Confirm with
  `ip route get 8.8.8.8`.
