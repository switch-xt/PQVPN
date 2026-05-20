#!/usr/bin/env bash
# gen_certs.sh — Generate self-signed TLS certificate for PQVPN server
#
# Produces:
#   server.crt      — PEM certificate (for server)
#   server.key      — PEM private key (for server)
#   server_cert.der — DER-encoded certificate (for client pinning)
#
# Uses EC P-256 key with 10-year validity.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${1:-$SCRIPT_DIR/../server/certs}"

mkdir -p "$OUT_DIR"

echo "=== Generating PQVPN TLS Certificate ==="
echo "Output directory: $OUT_DIR"

# Generate EC P-256 private key
openssl ecparam -genkey -name prime256v1 -noout -out "$OUT_DIR/server.key"
chmod 600 "$OUT_DIR/server.key"
echo "✓ Generated EC P-256 private key: server.key"

# Generate self-signed certificate (10-year validity = 3650 days)
openssl req -new -x509 \
    -key "$OUT_DIR/server.key" \
    -out "$OUT_DIR/server.crt" \
    -days 3650 \
    -subj "/CN=pqvpn-server/O=PQVPN/C=US" \
    -addext "subjectAltName=DNS:pqvpn-server,IP:0.0.0.0" \
    -addext "keyUsage=digitalSignature,keyAgreement" \
    -addext "extendedKeyUsage=serverAuth"

echo "✓ Generated self-signed certificate: server.crt (valid 10 years)"

# Export DER-encoded certificate for client certificate pinning
openssl x509 -in "$OUT_DIR/server.crt" -outform DER -out "$OUT_DIR/server_cert.der"
echo "✓ Exported DER certificate: server_cert.der (for client pinning)"

# Display certificate info
echo ""
echo "=== Certificate Details ==="
openssl x509 -in "$OUT_DIR/server.crt" -noout -subject -dates -fingerprint -sha256

# Display SHA-256 pin for client configuration
echo ""
echo "=== Certificate Pin (SHA-256) ==="
openssl x509 -in "$OUT_DIR/server.crt" -pubkey -noout | \
    openssl pkey -pubin -outform DER | \
    openssl dgst -sha256 -binary | \
    openssl enc -base64
echo ""
echo "Done! Copy server.crt and server.key to /etc/pqvpn/ on the server."
echo "Copy server_cert.der to the client for certificate pinning."
