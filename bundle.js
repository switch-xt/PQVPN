const fs = require('fs');
const path = require('path');

const filesToBundle = [
    "server/internal/api/handlers.go",
    "server/internal/wg/manager.go",
    "server/internal/psk/provider.go",
    "client/src-tauri/src/commands.rs",
    "client/src-tauri/src/wireguard/windows.rs",
    "client/src-tauri/src/pqc/psk_provider.rs",
    "client/src/App.tsx",
    "client/src/components/ModeSelector.tsx"
];

const desktopPath = "C:\\Users\\AYUSH KUMAR\\Desktop\\pqvpn_codebase_context.md";
const baseDir = "C:\\Users\\AYUSH KUMAR\\Desktop\\vpnX";

let out = "# PQVPN Codebase Context\n\n";
out += "This file contains the core logic for the PQVPN True Peer Relay implementation.\n";
out += "Current architecture:\n";
out += "- **Sharer (Exit Node)**: Windows machine that runs `Set-NetIPInterface -Forwarding Enabled` and `New-NetNat`. Its WireGuard `AllowedIPs` is restricted to `10.8.0.0/24` locally, but the Go Server grants it `0.0.0.0/0` so the server WireGuard interface doesn't drop incoming internet replies.\n";
out += "- **Peer (Client)**: Connects with a TargetCode. The Go Server executes `ip rule add from <PeerIP> table 100` and `ip route add default via <SharerIP> dev wg0 table 100`.\n";
out += "- **Go Server**: Handles the handshake, maps ShareCode to IPs, dynamically updates Linux routing rules, and configures WireGuard via `wg set`.\n\n";

for (const relPath of filesToBundle) {
    const fullPath = path.join(baseDir, relPath);
    if (fs.existsSync(fullPath)) {
        out += `## ${relPath}\n`;
        const ext = relPath.split('.').pop();
        out += `\`\`\`${ext}\n`;
        out += fs.readFileSync(fullPath, 'utf-8');
        out += `\n\`\`\`\n\n`;
    } else {
        out += `## ${relPath} (NOT FOUND)\n\n`;
    }
}

fs.writeFileSync(desktopPath, out, 'utf-8');
console.log(`Successfully bundled to ${desktopPath}`);
