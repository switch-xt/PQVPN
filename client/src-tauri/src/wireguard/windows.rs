//! WireGuard tunnel management for Windows.
//!
//! Uses `wireguard.exe` service control for tunnel installation and removal.
//! Configuration files are stored in `%LOCALAPPDATA%\PQVPN\tunnels\`.

use anyhow::{anyhow, Context, Result};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Windows flag to suppress console window creation.
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Parameters needed to render a WireGuard configuration.
#[derive(Debug, Clone)]
pub struct WgConfParams {
    /// Client WireGuard private key (base64).
    pub private_key: String,
    /// Client tunnel address (e.g. "10.0.0.2/32").
    pub address: String,
    /// DNS server(s) (e.g. "1.1.1.1, 8.8.8.8").
    pub dns: String,
    /// Server WireGuard public key (base64).
    pub server_pubkey: String,
    /// Optional 32-byte PresharedKey (base64).
    pub preshared_key: Option<String>,
    /// Server endpoint "host:port".
    pub endpoint: String,
    /// Allowed IPs (e.g. "0.0.0.0/0, ::/0" for full tunnel).
    pub allowed_ips: String,
    /// Whether to apply gaming-mode optimizations (lower MTU, faster keepalive).
    pub gaming_mode: bool,
}

/// Render a WireGuard configuration string from the given parameters.
pub fn render_conf(params: &WgConfParams) -> String {
    let psk_line = match &params.preshared_key {
        Some(psk) => format!("PresharedKey = {}\n", psk),
        None => "".to_string(),
    };

    let mtu_line = if params.gaming_mode { "MTU = 1280\n" } else { "" };
    let keepalive = if params.gaming_mode { 15 } else { 25 };

    format!(
        r#"[Interface]
PrivateKey = {private_key}
Address = {address}
DNS = {dns}
{mtu_line}
[Peer]
PublicKey = {server_pubkey}
{psk_line}Endpoint = {endpoint}
AllowedIPs = {allowed_ips}
PersistentKeepalive = {keepalive}
"#,
        private_key = params.private_key,
        address = params.address,
        dns = params.dns,
        mtu_line = mtu_line,
        server_pubkey = params.server_pubkey,
        psk_line = psk_line,
        endpoint = params.endpoint,
        allowed_ips = params.allowed_ips,
        keepalive = keepalive,
    )
}

/// Get the tunnel configuration directory: `%LOCALAPPDATA%\PQVPN\tunnels`.
fn tunnels_dir() -> Result<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .context("LOCALAPPDATA environment variable not set")?;
    let dir = PathBuf::from(local_app_data).join("PQVPN").join("tunnels");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create tunnel dir: {}", dir.display()))?;
    Ok(dir)
}

/// Write a WireGuard `.conf` file to the tunnels directory.
///
/// Returns the full path to the written configuration file.
pub fn write_conf(tunnel_name: &str, params: &WgConfParams) -> Result<PathBuf> {
    let dir = tunnels_dir()?;
    let conf_path = dir.join(format!("{tunnel_name}.conf"));
    let conf_content = render_conf(params);
    std::fs::write(&conf_path, conf_content)
        .with_context(|| format!("write conf: {}", conf_path.display()))?;
    Ok(conf_path)
}

/// Full path to wireguard.exe
fn wg_exe() -> Result<String> {
    Ok(r"C:\Program Files\WireGuard\wireguard.exe".to_string())
}

/// Bring a WireGuard tunnel up via a single elevated PowerShell script.
///
/// This writes a temporary `.ps1` script that:
/// 1. Checks if an existing tunnel service is running and removes it
/// 2. Installs the new tunnel service
/// All errors are suppressed (`2>$null`) so no native Windows error dialogs
/// appear, which would freeze the WebView2 event loop.
pub fn tunnel_up(conf_path: &Path) -> Result<()> {
    let exe = wg_exe()?;
    let conf_str = conf_path.to_string_lossy().to_string();
    let tunnel_name = conf_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "pqvpn0".to_string());

    // Write a temporary PowerShell script so we only trigger ONE UAC prompt
    let script_dir = conf_path.parent().unwrap_or(Path::new("."));
    let script_path = script_dir.join("_tunnel_up.ps1");

    let script_content = format!(
        r#"$ErrorActionPreference = 'SilentlyContinue'
$svc = Get-Service -Name 'WireGuardTunnel${name}' -ErrorAction SilentlyContinue
if ($svc) {{
    & '{exe}' /uninstalltunnelservice '{name}' 2>$null
    Start-Sleep -Seconds 2
}}
& '{exe}' /installtunnelservice '{conf}' 2>$null
"#,
        name = tunnel_name,
        exe = exe,
        conf = conf_str,
    );

    std::fs::write(&script_path, &script_content)
        .with_context(|| format!("write tunnel script: {}", script_path.display()))?;

    // Run the script elevated — single UAC prompt, no visible windows
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-WindowStyle", "Hidden",
            "-Command",
            &format!(
                "Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','\"{script}\"' -Verb RunAs -Wait -WindowStyle Hidden",
                script = script_path.to_string_lossy()
            ),
        ])
        .output()
        .context("failed to launch elevated PowerShell")?;

    // Clean up the temp script
    let _ = std::fs::remove_file(&script_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If the user declined UAC, stderr will mention "canceled by the user"
        if stderr.contains("canceled") || stderr.contains("cancelled") {
            return Err(anyhow!("UAC prompt was declined"));
        }
        return Err(anyhow!("tunnel install failed: {}", stderr.trim()));
    }

    // Give the service a moment to start
    std::thread::sleep(std::time::Duration::from_millis(3000));

    // Verify the tunnel actually came up
    if !tunnel_is_up(&tunnel_name) {
        return Err(anyhow!(
            "WireGuard service was installed but is not running. Check WireGuard logs."
        ));
    }

    Ok(())
}

/// Bring a WireGuard tunnel down via elevated `wireguard.exe /uninstalltunnelservice`.
pub fn tunnel_down(tunnel_name: &str) -> Result<()> {
    // Only attempt removal if the service actually exists
    if !tunnel_is_up(tunnel_name) {
        return Ok(());
    }

    let exe = wg_exe()?;

    let script_dir = std::env::var("LOCALAPPDATA")
        .map(|d| PathBuf::from(d).join("PQVPN").join("tunnels"))
        .unwrap_or_else(|_| PathBuf::from("."));
    let script_path = script_dir.join("_tunnel_down.ps1");

    let script_content = format!(
        r#"$ErrorActionPreference = 'SilentlyContinue'
& '{exe}' /uninstalltunnelservice '{name}' 2>$null
"#,
        exe = exe,
        name = tunnel_name,
    );

    std::fs::write(&script_path, &script_content)
        .with_context(|| format!("write tunnel script: {}", script_path.display()))?;

    let _output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-WindowStyle", "Hidden",
            "-Command",
            &format!(
                "Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','\"{script}\"' -Verb RunAs -Wait -WindowStyle Hidden",
                script = script_path.to_string_lossy()
            ),
        ])
        .output()
        .context("failed to launch elevated PowerShell")?;

    let _ = std::fs::remove_file(&script_path);
    Ok(())
}

/// Check if a WireGuard tunnel service is currently running via `sc query`.
pub fn tunnel_is_up(tunnel_name: &str) -> bool {
    let service_name = format!("WireGuardTunnel${tunnel_name}");
    let output = Command::new("sc")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["query", &service_name])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            // Service is running if the output contains "RUNNING"
            stdout.contains("RUNNING")
        }
        Err(_) => false,
    }
}

/// Convert a 32-byte PSK to the base64 string used in WireGuard configs.
pub fn psk_to_base64(psk: &[u8; 32]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(psk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_conf() {
        let params = WgConfParams {
            private_key: "YWJjZGVmZw==".into(),
            address: "10.0.0.2/32".into(),
            dns: "1.1.1.1".into(),
            server_pubkey: "c2VydmVycHVia2V5".into(),
            preshared_key: "aabbccdd".into(),
            endpoint: "1.2.3.4:51820".into(),
            allowed_ips: "0.0.0.0/0".into(),
            gaming_mode: false,
        };
        let conf = render_conf(&params);
        assert!(conf.contains("PrivateKey = YWJjZGVmZw=="));
        assert!(conf.contains("PresharedKey = aabbccdd"));
        assert!(conf.contains("Endpoint = 1.2.3.4:51820"));
        assert!(conf.contains("AllowedIPs = 0.0.0.0/0"));
        assert!(conf.contains("PersistentKeepalive = 25"));
    }

    #[test]
    fn test_psk_to_base64() {
        let psk = [0xab_u8; 32];
        let b64 = psk_to_base64(&psk);
        assert_eq!(b64.len(), 44);
        assert!(b64.ends_with('='));
    }
}
