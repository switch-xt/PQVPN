//! ML-KEM-768 PSK negotiation with a PQVPN server.
//!
//! This module handles the post-quantum key exchange using ML-KEM-768 (FIPS 203).
//! The negotiated shared secret is passed through HKDF-SHA256 to derive a 32-byte
//! WireGuard PresharedKey.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hkdf::Hkdf;
use ml_kem::{KemCore, MlKem768, EncodedSizeUser};
use ml_kem::kem::{Decapsulate, Encapsulate};
use rand::rngs::OsRng;
use rustls::pki_types::ServerName;
use sha2::Sha256;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

/// Info string used as the HKDF info parameter for PSK derivation.
const HKDF_INFO: &[u8] = b"pqvpn-psk-v1";

/// Result of a successful PSK negotiation with the server.
#[derive(Debug, Clone)]
pub struct Negotiated {
    /// 32-byte WireGuard PresharedKey (hex-encoded for .conf rendering).
    pub psk: [u8; 32],
    /// Server's WireGuard public key (base64).
    pub server_wg_pubkey: String,
    /// Server endpoint in "host:port" format for WireGuard.
    pub server_endpoint: String,
}

/// JSON payload sent to the server.
#[derive(serde::Serialize)]
struct ClientHello {
    ek_b64: String,
    wg_pubkey: String,
    auth_token: String,
    mode: String,
}

/// JSON payload received from the server.
#[derive(serde::Deserialize)]
struct ServerResponse {
    ct_b64: String,
    server_wg_pubkey: String,
    server_endpoint: String,
    hkdf_salt_b64: String,
}

/// Build a `rustls::ClientConfig` that pins a single server certificate.
///
/// `pinned_der` is the DER-encoded certificate bytes for the server.
fn make_tls_config(pinned_der: &[u8]) -> Result<Arc<rustls::ClientConfig>> {
    let cert = rustls::pki_types::CertificateDer::from(pinned_der.to_vec());
    let mut root_store = rustls::RootCertStore::empty();
    root_store
        .add(cert)
        .map_err(|e| anyhow!("failed to add pinned cert: {e}"))?;

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(Arc::new(config))
}

/// Perform a full ML-KEM-768 PSK negotiation with the server.
///
/// # Protocol
///
/// 1. Generate ephemeral ML-KEM-768 keypair `(dk, ek)`.
/// 2. Connect via TLS (with pinned cert) to `server_host:server_port`.
/// 3. Send `ClientHello` JSON (encapsulation key, WG pubkey, auth token) + newline.
/// 4. Read `ServerResponse` JSON containing ciphertext, server WG info, and HKDF salt.
/// 5. Decapsulate the ciphertext → shared secret.
/// 6. HKDF-SHA256(shared_secret, salt, "pqvpn-psk-v1") → 32-byte PSK.
/// 7. Return `Negotiated { psk, server_wg_pubkey, server_endpoint }`.
pub fn negotiate(
    server_host: &str,
    server_port: u16,
    pinned_cert_der: &[u8],
    wg_pubkey: &str,
    auth_token: &str,
    mode: &str,
) -> Result<Negotiated> {
    // --- Step 1: Generate ephemeral ML-KEM-768 keypair ---
    let (dk, ek) = MlKem768::generate(&mut OsRng);

    // Encode the encapsulation key to bytes, then base64
    let ek_bytes = ek.as_bytes();
    let ek_b64 = B64.encode(ek_bytes.as_slice());

    // --- Step 2: TLS connection with cert pinning ---
    let tls_config = make_tls_config(pinned_cert_der)?;
    let server_name = ServerName::try_from(server_host.to_owned())
        .map_err(|e| anyhow!("invalid server name: {e}"))?;

    let mut tcp_stream = std::net::TcpStream::connect((server_host, server_port))
        .context("TCP connect failed")?;

    let mut tls_conn = rustls::ClientConnection::new(tls_config, server_name)
        .context("TLS handshake init failed")?;

    let mut tls_stream = rustls::Stream::new(&mut tls_conn, &mut tcp_stream);

    // --- Step 3: Send ClientHello ---
    let hello = ClientHello {
        ek_b64,
        wg_pubkey: wg_pubkey.to_string(),
        auth_token: auth_token.to_string(),
        mode: mode.to_string(),
    };
    let mut hello_json = serde_json::to_string(&hello).context("serialise ClientHello")?;
    hello_json.push('\n');
    tls_stream
        .write_all(hello_json.as_bytes())
        .context("send ClientHello")?;
    tls_stream.flush().context("flush ClientHello")?;

    // --- Step 4: Read ServerResponse ---
    // We need to read from the TLS stream. Use a manual read loop since
    // rustls::Stream borrows mutably. Read until newline.
    let mut response_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        use std::io::Read;
        let n = tls_stream.read(&mut byte).context("read ServerResponse")?;
        if n == 0 {
            return Err(anyhow!("server closed connection before response"));
        }
        if byte[0] == b'\n' {
            break;
        }
        response_buf.push(byte[0]);
    }

    let resp: ServerResponse =
        serde_json::from_slice(&response_buf).context("parse ServerResponse JSON")?;

    // --- Step 5: Decapsulate ---
    let ct_bytes = B64
        .decode(&resp.ct_b64)
        .context("decode ciphertext base64")?;

    // Convert ct_bytes into the proper Ciphertext type
    let ct: ml_kem::Ciphertext<MlKem768> =
        ml_kem::array::Array::try_from(ct_bytes.as_slice())
            .map_err(|_| anyhow!("ciphertext length mismatch"))?;

    let shared_secret = dk.decapsulate(&ct)
        .map_err(|_| anyhow!("ML-KEM decapsulation failed"))?;

    // --- Step 6: HKDF-SHA256 → 32-byte PSK ---
    let salt = B64
        .decode(&resp.hkdf_salt_b64)
        .context("decode HKDF salt base64")?;

    let hk = Hkdf::<Sha256>::new(Some(&salt), shared_secret.as_slice());
    let mut psk = [0u8; 32];
    hk.expand(HKDF_INFO, &mut psk)
        .map_err(|e| anyhow!("HKDF expand failed: {e}"))?;

    Ok(Negotiated {
        psk,
        server_wg_pubkey: resp.server_wg_pubkey,
        server_endpoint: resp.server_endpoint,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the ML-KEM-768 encapsulate/decapsulate round-trip works
    /// and HKDF derivation produces consistent 32-byte PSKs on both sides.
    #[test]
    fn psk_agreement_roundtrip() {
        let mut rng = OsRng;

        // Simulate client side: generate keypair
        let (dk, ek) = MlKem768::generate(&mut rng);

        // Simulate server side: encapsulate to the client's encapsulation key
        let (ct, k_server) = ek.encapsulate(&mut rng).unwrap();

        // Client side: decapsulate
        let k_client = dk.decapsulate(&ct).unwrap();

        // Both sides should have the same shared secret
        assert_eq!(k_server.as_slice(), k_client.as_slice());

        // Derive PSK on both sides via HKDF
        let salt = b"test-salt-value-0123456789abcdef";

        let hk_server = Hkdf::<Sha256>::new(Some(salt), k_server.as_slice());
        let mut psk_server = [0u8; 32];
        hk_server.expand(HKDF_INFO, &mut psk_server).unwrap();

        let hk_client = Hkdf::<Sha256>::new(Some(salt), k_client.as_slice());
        let mut psk_client = [0u8; 32];
        hk_client.expand(HKDF_INFO, &mut psk_client).unwrap();

        assert_eq!(psk_server, psk_client);
        // PSK should not be all zeros
        assert_ne!(psk_server, [0u8; 32]);
    }
}
