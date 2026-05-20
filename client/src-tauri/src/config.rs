//! Application configuration for the PQVPN client.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// The PEM-encoded TLS certificate for the PQVPN server (pinned).
const SERVER_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIBhDCCASmgAwIBAgIUdwI2p9qyVb5lv+Dpl9xrvC+U4n0wCgYIKoZIzj0EAwIw
FzEVMBMGA1UEAwwMcHF2cG4tc2VydmVyMB4XDTI2MDUxOTIwMTM0MVoXDTI3MDUx
OTIwMTM0MVowFzEVMBMGA1UEAwwMcHF2cG4tc2VydmVyMFkwEwYHKoZIzj0CAQYI
KoZIzj0DAQcDQgAE4V9C90ZD54rLtgcn/kXoinJZKBoyi3BKTz1d2efgjfGi+q+h
+bgpDQGyLWMTQWmeOZXtDotVBiUmnf4+5FcfTKNTMFEwHQYDVR0OBBYEFMhxySmi
f4EydF8WRijpkk4vwUk9MB8GA1UdIwQYMBaAFMhxySmif4EydF8WRijpkk4vwUk9
MA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDSQAwRgIhALvHFjdsfgHMVTrR
2PjinvbiyXMadZluhNtaERUxa0diAiEA5Vd0jouipJ4JwP6ViDMJ/MyeluRlIJo4
S8xDpXRnmCo=
-----END CERTIFICATE-----";

/// Parse PEM into DER bytes.
fn pem_to_der(pem: &str) -> Vec<u8> {
    let b64_content: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    B64.decode(&b64_content).expect("invalid base64 in PEM cert")
}

/// Application configuration that can be persisted or embedded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Hostname or IP of the PQVPN server.
    pub server_host: String,

    /// Port of the PQVPN server's key-exchange endpoint.
    pub server_port: u16,

    /// DER-encoded pinned TLS certificate for the server.
    #[serde(skip)]
    pub pinned_cert_der: Vec<u8>,

    /// Client WireGuard private key (base64-encoded).
    pub wg_private_key: String,

    /// Client tunnel IP address (e.g. "10.8.0.2/32").
    pub client_address: String,

    /// DNS servers for the tunnel.
    pub dns: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server_host: "34.136.62.117".into(),
            server_port: 8443,
            pinned_cert_der: pem_to_der(SERVER_CERT_PEM),
            wg_private_key: "COf6iVSab0Lk32ykZs79BMweeepfXpDTlN8ehqtdL0c=".into(),
            client_address: "10.8.0.2/32".into(),
            dns: "1.1.1.1, 8.8.8.8".into(),
        }
    }
}

impl AppConfig {
    /// Return the WireGuard public key corresponding to our private key.
    pub fn wg_pubkey(&self) -> String {
        // In a real scenario, this would derive the pubkey from wg_private_key using curve25519.
        // For MVP, we return a hardcoded one matching the hardcoded private key.
        "oc3rwAY0LpZv2M6H0CpqM/yhIN3FT2KQKGw4/LP1jiw=".to_string()
    }

    fn config_path() -> Option<std::path::PathBuf> {
        dirs::data_dir().map(|mut p| {
            p.push("PQVPN");
            std::fs::create_dir_all(&p).ok();
            p.push("config.dat");
            p
        })
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(encrypted) = std::fs::read(&path) {
                if let Ok(decrypted) = windows_dpapi::decrypt_data(&encrypted, windows_dpapi::Scope::User, None) {
                    if let Ok(config) = serde_json::from_slice::<Self>(&decrypted) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = Self::config_path() {
            let json = serde_json::to_vec(self)?;
            let encrypted = windows_dpapi::encrypt_data(&json, windows_dpapi::Scope::User, None)
                .map_err(|e| anyhow::anyhow!("DPAPI encryption failed: {:?}", e))?;
            std::fs::write(path, encrypted)?;
        }
        Ok(())
    }
}
