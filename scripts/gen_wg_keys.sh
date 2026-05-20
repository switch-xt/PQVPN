#!/usr/bin/env bash
# gen_wg_keys.sh — Generate WireGuard key pairs for server and client
#
# Produces:
#   server.key / server.pub — Server WireGuard key pair
#   client.key / client.pub — Client WireGuard key pair

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${1:-$SCRIPT_DIR/../keys}"

mkdir -p "$OUT_DIR"

echo "=== Generating WireGuard Key Pairs ==="
echo "Output directory: $OUT_DIR"

# Generate server key pair
wg genkey | tee "$OUT_DIR/server.key" | wg pubkey > "$OUT_DIR/server.pub"
chmod 600 "$OUT_DIR/server.key"
echo "✓ Server private key: server.key"
echo "✓ Server public key:  server.pub"
echo "  Public key: $(cat "$OUT_DIR/server.pub")"

echo ""

# Generate client key pair
wg genkey | tee "$OUT_DIR/client.key" | wg pubkey > "$OUT_DIR/client.pub"
chmod 600 "$OUT_DIR/client.key"
echo "✓ Client private key: client.key"
echo "✓ Client public key:  client.pub"
echo "  Public key: $(cat "$OUT_DIR/client.pub")"

echo ""
echo "Done!"
echo "Copy server.key and server.pub to /etc/wireguard/ on the server."
echo "Copy client.key and client.pub to the client device."
