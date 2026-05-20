// client/src-tauri/src/wireguard/windows.rs
//
// Programmatic WireGuard control on Windows. The official WireGuard for Windows
// installs each tunnel as a Windows service driven by the wireguard-nt kernel
// driver. `wg-quick` DOES NOT EXIST on Windows — that path in the original spec
// would never have worked. We use the documented tunnel-service mechanism.
//
// Bundle `wireguard.exe` (from the official MSI / embeddable build) as a Tauri
// sidecar resource. Installing a tunnel service requires Administrator; ship a
// UAC-elevated installer (expected for any VPN).
//
// Cargo deps: anyhow = "1"
//
// This module is pure orchestration. It receives the PSK from psk_provider.rs
// and never does crypto itself. Bring-up sequence:
//   negotiate() -> Negotiated -> write_conf() -> tunnel_up()

use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct TunnelParams<'a> {
    pub client_private_key_b64: &'a str,
    pub client_address_cidr: &'a str, // e.g. "10.8.0.2/24"
    pub dns: &'a str,                 // e.g. "1.1.1.1"
    pub server_pubkey_b64: &'a str,
    pub server_endpoint: &'a str,     // "ip:51820"
    pub psk_b64: &'a str,             // from psk_provider::psk_to_wg_b64
    pub mtu: u32,                     // 1420 default, 1280 for some ISPs
}

/// Render a wireguard-nt config. The PreSharedKey line is what makes this
/// tunnel post-quantum: the value came from the ML-KEM negotiation.
pub fn render_conf(p: &TunnelParams) -> String {
    format!(
        "[Interface]\n\
         PrivateKey = {priv}\n\
         Address = {addr}\n\
         DNS = {dns}\n\
         MTU = {mtu}\n\
         \n\
         [Peer]\n\
         PublicKey = {spub}\n\
         PresharedKey = {psk}\n\
         Endpoint = {ep}\n\
         AllowedIPs = 0.0.0.0/0\n\
         PersistentKeepalive = 25\n",
        priv = p.client_private_key_b64,
        addr = p.client_address_cidr,
        dns = p.dns,
        mtu = p.mtu,
        spub = p.server_pubkey_b64,
        psk = p.psk_b64,
        ep = p.server_endpoint,
    )
}

fn conf_dir() -> Result<PathBuf> {
    // Per-user app data; locked down. The conf holds the private key + PSK so
    // treat it as secret: write 0600-equivalent ACL in production hardening.
    let base = std::env::var("LOCALAPPDATA").context("LOCALAPPDATA unset")?;
    let dir = Path::new(&base).join("PQVPN").join("tunnels");
    std::fs::create_dir_all(&dir).context("create tunnel dir")?;
    Ok(dir)
}

/// Write the .conf for tunnel `name` (service name derives from the filename).
pub fn write_conf(name: &str, params: &TunnelParams) -> Result<PathBuf> {
    let path = conf_dir()?.join(format!("{name}.conf"));
    std::fs::write(&path, render_conf(params)).context("write conf")?;
    Ok(path)
}

fn wireguard_exe() -> Result<PathBuf> {
    // In production resolve the Tauri sidecar path. For Step 2 bring-up you can
    // hardcode the installed path:
    //   C:\Program Files\WireGuard\wireguard.exe
    let candidates = [
        r"C:\Program Files\WireGuard\wireguard.exe",
        r"C:\Program Files (x86)\WireGuard\wireguard.exe",
    ];
    for c in candidates {
        if Path::new(c).exists() {
            return Ok(PathBuf::from(c));
        }
    }
    Err(anyhow!(
        "wireguard.exe not found; install WireGuard for Windows or bundle the sidecar"
    ))
}

/// Bring the tunnel UP by installing it as a Windows service. Requires admin.
pub fn tunnel_up(conf_path: &Path) -> Result<()> {
    let status = Command::new(wireguard_exe()?)
        .arg("/installtunnelservice")
        .arg(conf_path)
        .status()
        .context("spawn wireguard /installtunnelservice")?;
    if !status.success() {
        return Err(anyhow!(
            "tunnel install failed (exit {:?}) — admin rights required",
            status.code()
        ));
    }
    Ok(())
}

/// Bring the tunnel DOWN by uninstalling its service. `name` = conf file stem.
pub fn tunnel_down(name: &str) -> Result<()> {
    let status = Command::new(wireguard_exe()?)
        .arg("/uninstalltunnelservice")
        .arg(name)
        .status()
        .context("spawn wireguard /uninstalltunnelservice")?;
    if !status.success() {
        return Err(anyhow!(
            "tunnel uninstall failed (exit {:?})",
            status.code()
        ));
    }
    Ok(())
}

/// True if the tunnel service for `name` is registered (rough liveness check).
pub fn tunnel_is_up(name: &str) -> bool {
    Command::new("sc")
        .args(["query", &format!("WireGuardTunnel${name}")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
        .unwrap_or(false)
}

// STEP 2 GATE manual test (run elevated):
//   1. write_conf("pqvpn0", &params)   // params from your Step-1 hand keys
//   2. tunnel_up(&path)                // browser shows GCP IP, ping works
//   3. assert tunnel_is_up("pqvpn0")
//   4. tunnel_down("pqvpn0")           // connectivity reverts
// If this passes, Rust controls the data plane and Step 3 can wire the PSK.
