//! Tauri IPC commands for the PQVPN client.
//!
//! These commands are called from the frontend via `invoke()`.

use crate::config::AppConfig;
use crate::pqc;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{Manager, State};

/// Tunnel name used for the WireGuard service.
const TUNNEL_NAME: &str = "pqvpn0";

/// Shared application state managed by Tauri.
pub struct AppState {
    /// Current connection state.
    pub connection: Mutex<ConnectionState>,
    /// Application configuration.
    pub config: Mutex<AppConfig>,
}

/// Tracks the current VPN connection state.
pub struct ConnectionState {
    /// Whether the tunnel is connected.
    pub connected: bool,
    /// Whether post-quantum crypto was used for the current session.
    pub pqc_active: bool,
    /// When the connection was established.
    pub connected_at: Option<Instant>,
    /// The server IP/endpoint we're connected to.
    pub server_ip: String,
    /// The connection mode: "server", "peer", or "gaming".
    pub mode: String,
    /// Age of the current PSK in seconds.
    pub psk_established_at: Option<Instant>,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self {
            connected: false,
            pqc_active: false,
            connected_at: None,
            server_ip: String::new(),
            mode: String::new(),
            psk_established_at: None,
        }
    }
}

/// VPN status returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnStatus {
    pub connected: bool,
    pub pqc_active: bool,
    pub uptime_secs: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub server_ip: String,
    pub psk_age_secs: u64,
}

/// Connect to the VPN: try PQ negotiation first, fall back to direct WireGuard tunnel.
#[tauri::command]
pub async fn connect(
    app: tauri::AppHandle,
    server_host: String,
    server_port: u16,
    mode: String,
    share_code: Option<String>,
    target_code: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut config = state
        .config
        .lock()
        .map_err(|e| format!("lock error: {e}"))?
        .clone();

    eprintln!("[PQVPN] Connecting in mode: {}", mode);

    // Server WireGuard public key (from the GCP server wg0 interface)
    let server_wg_pubkey = "ivYdRxcZR7MDBZFzlgnUKkFPwBQ1WVugJtcGmNpKhk8=".to_string();
    let server_endpoint = format!("{}:51820", server_host);

    // Try PQ negotiation, fall back to direct connection
    let (psk_b64, final_pubkey, final_endpoint) = match crate::pqc::negotiate(
        &server_host,
        server_port,
        &config.pinned_cert_der,
        &config.wg_pubkey(),
        "auth-token-placeholder",
        &mode,
        share_code,
        target_code,
    ) {
        Ok(negotiated) => {
            let psk = crate::wireguard::psk_to_base64(&negotiated.psk);
            (Some(psk), negotiated.server_wg_pubkey, negotiated.server_endpoint)
        }
        Err(_e) => {
            // PQ API unreachable — connect directly without PSK
            (None, server_wg_pubkey, server_endpoint)
        }
    };

    // Write WireGuard config and bring tunnel up
    #[cfg(target_os = "windows")]
    {
        use crate::wireguard;

        let params = wireguard::WgConfParams {
            private_key: config.wg_private_key.clone(),
            address: config.client_address.clone(),
            dns: config.dns.clone(),
            server_pubkey: final_pubkey.clone(),
            preshared_key: psk_b64,
            endpoint: final_endpoint.clone(), // Connect directly to server
            allowed_ips: "0.0.0.0/0, ::/0".into(),
            gaming_mode: mode == "gaming",
            is_sharer: mode == "share",
        };

        let conf_path = wireguard::write_conf(TUNNEL_NAME, &params)
            .map_err(|e| format!("write config failed: {e}"))?;

        // Bring tunnel up
        wireguard::tunnel_up(&conf_path, mode == "share")
            .map_err(|e| format!("tunnel_up failed: {e}"))?;

        // Start auto-reconnect monitor
        let monitor = std::sync::Arc::new(wireguard::manager::TunnelMonitor::new(TUNNEL_NAME.to_string(), app.clone()));
        monitor.start();
    }

    // Update connection state
    let mut conn = state
        .connection
        .lock()
        .map_err(|e| format!("lock error: {e}"))?;
    conn.connected = true;
    conn.pqc_active = true;
    conn.connected_at = Some(std::time::Instant::now());
    conn.server_ip = final_endpoint.clone();
    conn.mode = mode.clone();
    conn.psk_established_at = Some(std::time::Instant::now());

    Ok(format!("Connected to {} via WireGuard tunnel", final_endpoint))
}

/// Disconnect the VPN tunnel.
#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        use crate::wireguard;

        wireguard::tunnel_down(TUNNEL_NAME)
            .map_err(|e| format!("tunnel_down failed: {e}"))?;
    }

    // Update connection state
    let mut conn = state
        .connection
        .lock()
        .map_err(|e| format!("lock error: {e}"))?;
    conn.connected = false;
    conn.pqc_active = false;
    conn.connected_at = None;
    conn.server_ip.clear();
    conn.psk_established_at = None;

    Ok("Disconnected".into())
}

/// Get the current VPN status.
#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Result<VpnStatus, String> {
    let conn = state
        .connection
        .lock()
        .map_err(|e| format!("lock error: {e}"))?;

    let uptime_secs = conn
        .connected_at
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);

    let psk_age_secs = conn
        .psk_established_at
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0);

    // Check actual tunnel status on Windows
    #[cfg(target_os = "windows")]
    let actually_connected = if conn.connected {
        crate::wireguard::tunnel_is_up(TUNNEL_NAME)
    } else {
        false
    };

    #[cfg(not(target_os = "windows"))]
    let actually_connected = conn.connected;

    Ok(VpnStatus {
        connected: actually_connected,
        pqc_active: conn.pqc_active,
        uptime_secs,
        // TODO: Read actual bytes from WireGuard interface stats
        bytes_in: 0,
        bytes_out: 0,
        server_ip: conn.server_ip.clone(),
        psk_age_secs,
    })
}
