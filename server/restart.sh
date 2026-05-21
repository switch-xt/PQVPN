#!/bin/bash
sudo killall pqvpnd || true
mv pqvpnd-linux pqvpnd
chmod +x pqvpnd
WG_PUB=$(sudo cat /etc/wireguard/server.pub)
EXT_IP=$(curl -s http://metadata.google.internal/computeMetadata/v1/instance/network-interfaces/0/access-configs/0/external-ip -H 'Metadata-Flavor: Google')
nohup sudo ./pqvpnd --listen=:8443 --wg-pubkey-file=/etc/wireguard/server.pub --endpoint="${EXT_IP}:51820" --cert=certs/server.crt --key=certs/server.key --db=peers.db > pqvpnd.log 2>&1 &
