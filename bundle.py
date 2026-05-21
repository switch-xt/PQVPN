import os

files_to_bundle = [
    "server/internal/api/handlers.go",
    "server/internal/wg/manager.go",
    "server/internal/psk/provider.go",
    "client/src-tauri/src/commands.rs",
    "client/src-tauri/src/wireguard/windows.rs",
    "client/src-tauri/src/pqc/psk_provider.rs",
    "client/src/App.tsx",
    "client/src/components/ModeSelector.tsx"
]

desktop_path = r"C:\Users\AYUSH KUMAR\Desktop\pqvpn_codebase_context.md"
base_dir = r"C:\Users\AYUSH KUMAR\Desktop\vpnX"

with open(desktop_path, "w", encoding="utf-8") as out:
    out.write("# PQVPN Codebase Context\n\n")
    out.write("This file contains the core logic for the PQVPN True Peer Relay implementation.\n")
    out.write("Current architecture:\n")
    out.write("- **Sharer (Exit Node)**: Windows machine that runs `Set-NetIPInterface -Forwarding Enabled` and `New-NetNat`. Its WireGuard `AllowedIPs` is restricted to `10.8.0.0/24` locally, but the Go Server grants it `0.0.0.0/0` so the server WireGuard interface doesn't drop incoming internet replies.\n")
    out.write("- **Peer (Client)**: Connects with a TargetCode. The Go Server executes `ip rule add from <PeerIP> table 100` and `ip route add default via <SharerIP> dev wg0 table 100`.\n")
    out.write("- **Go Server**: Handles the handshake, maps ShareCode to IPs, dynamically updates Linux routing rules, and configures WireGuard via `wg set`.\n\n")
    
    for rel_path in files_to_bundle:
        full_path = os.path.join(base_dir, rel_path.replace("/", "\\"))
        if os.path.exists(full_path):
            out.write(f"## {rel_path}\n")
            ext = rel_path.split('.')[-1]
            out.write(f"```{ext}\n")
            with open(full_path, "r", encoding="utf-8") as f:
                out.write(f.read())
            out.write(f"\n```\n\n")
        else:
            out.write(f"## {rel_path} (NOT FOUND)\n\n")

print(f"Successfully bundled to {desktop_path}")
