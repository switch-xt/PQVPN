#!/usr/bin/env bash
# setup_server.sh — Full server provisioning for PQVPN on Ubuntu 22.04
#
# This script:
#   1. Installs WireGuard and configures wg0
#   2. Installs Go 1.24+
#   3. Builds and installs pqvpnd
#   4. Generates TLS certs and WG keys
#   5. Enables IP forwarding + NAT (iptables)
#   6. Starts all services
#
# Usage: sudo bash setup_server.sh [SERVER_PUBLIC_IP]

set -euo pipefail

# ─── Configuration ───────────────────────────────────────────────────────────

GO_VERSION="1.24.3"
WG_IFACE="wg0"
WG_PORT="51820"
VPN_SUBNET="10.8.0.0/16"
VPN_SERVER_IP="10.8.0.1/16"
PQVPN_PORT="8443"

# Detect server public IP
SERVER_IP="${1:-$(curl -s4 ifconfig.me || echo "YOUR_IP")}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# ─── Preflight ───────────────────────────────────────────────────────────────

if [[ $EUID -ne 0 ]]; then
    echo "Error: This script must be run as root (use sudo)"
    exit 1
fi

echo "======================================"
echo "  PQVPN Server Setup — Ubuntu 22.04"
echo "======================================"
echo ""
echo "Server IP:    $SERVER_IP"
echo "WG Interface: $WG_IFACE"
echo "WG Port:      $WG_PORT"
echo "VPN Subnet:   $VPN_SUBNET"
echo "PQVPN Port:   $PQVPN_PORT"
echo ""

# ─── Step 1: System Updates & WireGuard ──────────────────────────────────────

echo "=== Step 1: Installing system packages ==="
apt-get update -qq
apt-get install -y wireguard wireguard-tools iptables curl

echo "✓ WireGuard installed"

# ─── Step 2: Install Go 1.24+ ───────────────────────────────────────────────

echo ""
echo "=== Step 2: Installing Go $GO_VERSION ==="

if command -v go &>/dev/null && go version | grep -q "go$GO_VERSION"; then
    echo "✓ Go $GO_VERSION already installed"
else
    GO_ARCHIVE="go${GO_VERSION}.linux-amd64.tar.gz"
    curl -fsSL "https://go.dev/dl/${GO_ARCHIVE}" -o "/tmp/${GO_ARCHIVE}"
    rm -rf /usr/local/go
    tar -C /usr/local -xzf "/tmp/${GO_ARCHIVE}"
    rm -f "/tmp/${GO_ARCHIVE}"
    
    # Add to path for this script
    export PATH="/usr/local/go/bin:$PATH"
    
    # Add to system profile
    if ! grep -q '/usr/local/go/bin' /etc/profile.d/go.sh 2>/dev/null; then
        echo 'export PATH="/usr/local/go/bin:$PATH"' > /etc/profile.d/go.sh
    fi
    
    echo "✓ Go $(go version) installed"
fi

# ─── Step 3: Generate WireGuard Keys ────────────────────────────────────────

echo ""
echo "=== Step 3: Generating WireGuard keys ==="

WG_DIR="/etc/wireguard"
mkdir -p "$WG_DIR"

if [[ ! -f "$WG_DIR/server.key" ]]; then
    wg genkey | tee "$WG_DIR/server.key" | wg pubkey > "$WG_DIR/server.pub"
    chmod 600 "$WG_DIR/server.key"
    echo "✓ Generated server WireGuard key pair"
else
    echo "✓ Server WireGuard keys already exist"
fi

SERVER_WG_PRIVKEY=$(cat "$WG_DIR/server.key")
SERVER_WG_PUBKEY=$(cat "$WG_DIR/server.pub")
echo "  Server WG Pubkey: $SERVER_WG_PUBKEY"

# ─── Step 4: Configure WireGuard Interface ──────────────────────────────────

echo ""
echo "=== Step 4: Configuring WireGuard interface ==="

WG_CONF="$WG_DIR/$WG_IFACE.conf"

if [[ ! -f "$WG_CONF" ]]; then
    cat > "$WG_CONF" << EOF
[Interface]
PrivateKey = $SERVER_WG_PRIVKEY
Address = $VPN_SERVER_IP
ListenPort = $WG_PORT
SaveConfig = false
EOF
    chmod 600 "$WG_CONF"
    echo "✓ Created $WG_CONF"
else
    echo "✓ WireGuard config already exists"
fi

# Bring up the interface
if ! ip link show "$WG_IFACE" &>/dev/null; then
    wg-quick up "$WG_IFACE"
    echo "✓ WireGuard interface $WG_IFACE is up"
else
    echo "✓ WireGuard interface $WG_IFACE already up"
fi

# Enable on boot
systemctl enable "wg-quick@${WG_IFACE}" 2>/dev/null || true

# ─── Step 5: Enable IP Forwarding & NAT ─────────────────────────────────────

echo ""
echo "=== Step 5: Configuring IP forwarding & NAT ==="

# Enable IP forwarding
if ! grep -q "^net.ipv4.ip_forward=1" /etc/sysctl.conf; then
    echo "net.ipv4.ip_forward=1" >> /etc/sysctl.conf
fi
sysctl -w net.ipv4.ip_forward=1 > /dev/null

echo "✓ IP forwarding enabled"

# Detect the default outbound interface
DEFAULT_IFACE=$(ip route show default | awk '{print $5}' | head -1)

# Add NAT masquerade rule (idempotent)
if ! iptables -t nat -C POSTROUTING -s "$VPN_SUBNET" -o "$DEFAULT_IFACE" -j MASQUERADE 2>/dev/null; then
    iptables -t nat -A POSTROUTING -s "$VPN_SUBNET" -o "$DEFAULT_IFACE" -j MASQUERADE
fi

# Allow forwarding for VPN traffic
if ! iptables -C FORWARD -i "$WG_IFACE" -j ACCEPT 2>/dev/null; then
    iptables -A FORWARD -i "$WG_IFACE" -j ACCEPT
fi
if ! iptables -C FORWARD -o "$WG_IFACE" -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null; then
    iptables -A FORWARD -o "$WG_IFACE" -m state --state RELATED,ESTABLISHED -j ACCEPT
fi

echo "✓ NAT masquerade configured (via $DEFAULT_IFACE)"

# Persist iptables rules
if command -v netfilter-persistent &>/dev/null; then
    netfilter-persistent save
elif command -v iptables-save &>/dev/null; then
    iptables-save > /etc/iptables.rules
    # Add restore on boot
    if [[ ! -f /etc/network/if-pre-up.d/iptables ]]; then
        cat > /etc/network/if-pre-up.d/iptables << 'IPTEOF'
#!/bin/sh
iptables-restore < /etc/iptables.rules
IPTEOF
        chmod +x /etc/network/if-pre-up.d/iptables
    fi
fi

echo "✓ iptables rules persisted"

# ─── Step 6: Generate TLS Certificates ──────────────────────────────────────

echo ""
echo "=== Step 6: Generating TLS certificates ==="

CERT_DIR="/etc/pqvpn"
mkdir -p "$CERT_DIR"

if [[ ! -f "$CERT_DIR/server.crt" ]]; then
    # Generate EC P-256 key
    openssl ecparam -genkey -name prime256v1 -noout -out "$CERT_DIR/server.key"
    chmod 600 "$CERT_DIR/server.key"

    # Generate self-signed cert (10 years)
    openssl req -new -x509 \
        -key "$CERT_DIR/server.key" \
        -out "$CERT_DIR/server.crt" \
        -days 3650 \
        -subj "/CN=pqvpn-server/O=PQVPN/C=US" \
        -addext "subjectAltName=DNS:pqvpn-server,IP:$SERVER_IP" \
        -addext "keyUsage=digitalSignature,keyAgreement" \
        -addext "extendedKeyUsage=serverAuth"

    # Export DER for client pinning
    openssl x509 -in "$CERT_DIR/server.crt" -outform DER -out "$CERT_DIR/server_cert.der"

    echo "✓ Generated TLS certificate (EC P-256, 10-year validity)"
    echo "  Certificate: $CERT_DIR/server.crt"
    echo "  Private key: $CERT_DIR/server.key"
    echo "  DER (pin):   $CERT_DIR/server_cert.der"
else
    echo "✓ TLS certificates already exist"
fi

# ─── Step 7: Build & Install pqvpnd ─────────────────────────────────────────

echo ""
echo "=== Step 7: Building pqvpnd ==="

cd "$PROJECT_DIR/server"

# Download dependencies
/usr/local/go/bin/go mod download
echo "✓ Go dependencies downloaded"

# Build the daemon
/usr/local/go/bin/go build -o /usr/local/bin/pqvpnd ./cmd/pqvpnd/
chmod +x /usr/local/bin/pqvpnd
echo "✓ Built and installed /usr/local/bin/pqvpnd"

# ─── Step 8: Setup systemd Service ──────────────────────────────────────────

echo ""
echo "=== Step 8: Installing systemd service ==="

# Create data directory
mkdir -p /var/lib/pqvpn
chmod 750 /var/lib/pqvpn

# Install the service file with the real server IP substituted
sed "s/YOUR_IP/$SERVER_IP/g" "$PROJECT_DIR/server/pqvpnd.service" > /etc/systemd/system/pqvpnd.service

systemctl daemon-reload
systemctl enable pqvpnd
systemctl start pqvpnd

echo "✓ pqvpnd service installed and started"

# ─── Step 9: Firewall (UFW) ─────────────────────────────────────────────────

echo ""
echo "=== Step 9: Configuring firewall ==="

if command -v ufw &>/dev/null; then
    ufw allow "$WG_PORT/udp" comment "WireGuard"
    ufw allow "$PQVPN_PORT/tcp" comment "PQVPN API"
    echo "✓ UFW rules added for WireGuard ($WG_PORT/udp) and PQVPN ($PQVPN_PORT/tcp)"
else
    echo "⚠ UFW not installed — ensure ports $WG_PORT/udp and $PQVPN_PORT/tcp are open"
fi

# ─── Done ────────────────────────────────────────────────────────────────────

echo ""
echo "======================================"
echo "  PQVPN Server Setup Complete! 🎉"
echo "======================================"
echo ""
echo "Server details:"
echo "  Public IP:           $SERVER_IP"
echo "  WireGuard endpoint:  $SERVER_IP:$WG_PORT"
echo "  PQVPN API:           https://$SERVER_IP:$PQVPN_PORT"
echo "  WG Public Key:       $SERVER_WG_PUBKEY"
echo ""
echo "Client needs:"
echo "  1. server_cert.der from $CERT_DIR/server_cert.der"
echo "  2. Server WG pubkey: $SERVER_WG_PUBKEY"
echo "  3. Server endpoint:  $SERVER_IP:$WG_PORT"
echo "  4. API endpoint:     https://$SERVER_IP:$PQVPN_PORT"
echo ""
echo "Check status:"
echo "  systemctl status pqvpnd"
echo "  journalctl -u pqvpnd -f"
echo "  wg show $WG_IFACE"
