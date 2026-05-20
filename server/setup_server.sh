#!/bin/bash
set -e

echo "=== PQVPN Server Setup ==="

# 1. WireGuard config
echo "[1/4] Writing WireGuard config..."
PRIVKEY=$(sudo cat /etc/wireguard/server.key)
sudo bash -c "cat > /etc/wireguard/wg0.conf << EOF
[Interface]
Address = 10.8.0.1/24
ListenPort = 51820
PrivateKey = ${PRIVKEY}
PostUp = iptables -A FORWARD -i wg0 -j ACCEPT; iptables -t nat -A POSTROUTING -o ens4 -j MASQUERADE; sysctl -w net.ipv4.ip_forward=1
PostDown = iptables -D FORWARD -i wg0 -j ACCEPT; iptables -t nat -D POSTROUTING -o ens4 -j MASQUERADE
EOF"
sudo chmod 600 /etc/wireguard/wg0.conf
echo "WireGuard config written."

# 2. Start WireGuard
echo "[2/4] Starting WireGuard..."
sudo wg-quick up wg0
echo "WireGuard is UP."

# 3. Generate TLS certs for the PQ key exchange API
echo "[3/4] Generating TLS certificates..."
mkdir -p ~/certs
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
  -keyout ~/certs/server.key -out ~/certs/server.crt \
  -days 365 -nodes -subj '/CN=pqvpn-server' 2>&1
echo "TLS certs generated."

# 4. Start the PQVPN daemon in background
echo "[4/4] Starting PQVPN daemon..."
WG_PUB=$(sudo cat /etc/wireguard/server.pub)
EXT_IP=$(curl -s http://metadata.google.internal/computeMetadata/v1/instance/network-interfaces/0/access-configs/0/external-ip -H "Metadata-Flavor: Google")

nohup ~/pqvpnd \
  --listen=:8443 \
  --wg-pubkey-file=/etc/wireguard/server.pub \
  --endpoint="${EXT_IP}:51820" \
  --cert=~/certs/server.crt \
  --key=~/certs/server.key \
  --db=~/peers.db \
  > ~/pqvpnd.log 2>&1 &

sleep 2

echo ""
echo "========================================="
echo "  PQVPN SERVER IS RUNNING"
echo "========================================="
echo "  External IP:    ${EXT_IP}"
echo "  WG Public Key:  ${WG_PUB}"
echo "  API Port:       8443"
echo "  WG Port:        51820"
echo "========================================="
