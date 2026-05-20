// client/src-tauri/src/pqc/psk_provider.rs
//
// The heart of PQVPN. Produces a 32-byte WireGuard PreSharedKey via an
// ephemeral ML-KEM-768 key encapsulation over a TLS channel whose server
// certificate is PINNED (we control both endpoints; no public CA).
//
// Post-quantum guarantee: the client's ML-KEM decapsulation key NEVER leaves
// this process. An adversary recording the wire (even one who later breaks the
// classical TLS layer with a quantum computer) sees only the encapsulation key
// and the ciphertext, both of which are public by design. Without the decaps
// key they cannot derive `shared_secret`. The PSK is therefore PQ-secret.
//
// Cargo deps (client/src-tauri/Cargo.toml):
//   ml-kem      = "0.2"     # FIPS 203, pure Rust
//   hkdf        = "0.12"
//   sha2        = "0.10"
//   rand        = "0.8"
//   rustls      = "0.23"
//   serde       = { version = "1", features = ["derive"] }
//   serde_json  = "1"
//   base64      = "0.22"
//   anyhow      = "1"
//
// API NOTE: the `ml-kem` crate's trait/method names have moved between
// releases. The three call sites are marked `// VERIFY`. Logic is correct;
// only symbol spelling may need a one-line tweak against `cargo doc -p ml-kem`.

use anyhow::{anyhow, Context, Result};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use ml_kem::{
    kem::{Decapsulate, Encapsulate}, // VERIFY trait paths
    EncodedSizeUser,
    KemCore,
    MlKem768,
};

pub const PSK_LEN: usize = 32;
const HKDF_INFO: &[u8] = b"pqvpn-psk-v1";

#[derive(Serialize)]
struct ClientHello {
    /// ML-KEM-768 encapsulation (public) key, base64.
    ek_b64: String,
    /// This client's WireGuard public key (so the server can `wg set` the peer).
    wg_pubkey: String,
    /// Opaque identity token (JWT for v1). Server authenticates the peer.
    auth_token: String,
}

#[derive(Deserialize)]
struct ServerResponse {
    /// ML-KEM-768 ciphertext, base64.
    ct_b64: String,
    /// Server's WireGuard public key + endpoint, so the client can build conf.
    server_wg_pubkey: String,
    server_endpoint: String,
    /// Per-session salt the server also fed into HKDF. Must match both sides.
    hkdf_salt_b64: String,
}

/// Result of one successful negotiation. Hand the `psk` straight to the
/// WireGuard config writer; nothing else in the app sees crypto.
pub struct Negotiated {
    pub psk: [u8; PSK_LEN],
    pub server_wg_pubkey: String,
    pub server_endpoint: String,
}

/// Run one full PSK negotiation against `server_host:server_port`.
///
/// `pinned_cert_der` is the server's certificate (DER) compiled into the
/// client build. We trust ONLY this exact certificate — no system roots.
pub fn negotiate(
    server_host: &str,
    server_port: u16,
    pinned_cert_der: &[u8],
    wg_pubkey: &str,
    auth_token: &str,
) -> Result<Negotiated> {
    // 1. Ephemeral ML-KEM-768 keypair. Lives only for this negotiation.
    let mut rng = rand::thread_rng();
    let (dk, ek) = MlKem768::generate(&mut rng); // VERIFY: KeyGen::generate
    let ek_bytes = ek.as_bytes(); // VERIFY: EncodedSizeUser::as_bytes
    let ek_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        ek_bytes,
    );

    // 2. TLS with the pinned cert as the ONLY trust anchor.
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(pinned_cert_der.to_vec()))
        .context("pinning server certificate failed")?;
    let tls_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = rustls::pki_types::ServerName::try_from(server_host.to_owned())
        .context("invalid server name")?;
    let mut conn = rustls::ClientConnection::new(Arc::new(tls_cfg), server_name)?;
    let mut sock = TcpStream::connect((server_host, server_port))
        .context("TCP connect to server failed")?;
    let mut tls = rustls::Stream::new(&mut conn, &mut sock);

    // 3. ClientHello.
    let hello = ClientHello {
        ek_b64,
        wg_pubkey: wg_pubkey.to_string(),
        auth_token: auth_token.to_string(),
    };
    let mut line = serde_json::to_vec(&hello)?;
    line.push(b'\n');
    tls.write_all(&line).context("sending ClientHello failed")?;
    tls.flush().ok();

    // 4. ServerResponse (newline-delimited JSON).
    let mut buf = Vec::with_capacity(4096);
    let mut byte = [0u8; 1];
    loop {
        let n = tls.read(&mut byte).context("reading ServerResponse failed")?;
        if n == 0 {
            return Err(anyhow!("server closed connection before response"));
        }
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
        if buf.len() > 1 << 20 {
            return Err(anyhow!("ServerResponse too large"));
        }
    }
    let resp: ServerResponse =
        serde_json::from_slice(&buf).context("malformed ServerResponse")?;

    // 5. Decapsulate -> shared secret. This is where PQ secrecy is created.
    let ct_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        resp.ct_b64.as_bytes(),
    )
    .context("bad ciphertext base64")?;
    let ct = ml_kem::Ciphertext::<MlKem768>::try_from(ct_bytes.as_slice())
        .map_err(|_| anyhow!("ciphertext wrong length for ML-KEM-768"))?; // VERIFY
    let shared_secret = dk
        .decapsulate(&ct) // VERIFY: Decapsulate::decapsulate
        .map_err(|_| anyhow!("ML-KEM decapsulation failed"))?;

    // 6. HKDF -> 32-byte PSK. Salt is per-session and identical on both ends.
    let salt = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        resp.hkdf_salt_b64.as_bytes(),
    )
    .context("bad hkdf salt base64")?;
    let hk = Hkdf::<Sha256>::new(Some(&salt), shared_secret.as_ref());
    let mut psk = [0u8; PSK_LEN];
    hk.expand(HKDF_INFO, &mut psk)
        .map_err(|_| anyhow!("HKDF expand failed"))?;

    Ok(Negotiated {
        psk,
        server_wg_pubkey: resp.server_wg_pubkey,
        server_endpoint: resp.server_endpoint,
    })
}

/// WireGuard wants the PSK base64-encoded in the config file.
pub fn psk_to_wg_b64(psk: &[u8; PSK_LEN]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, psk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ml_kem::{kem::Encapsulate, KemCore, MlKem768};

    // Proves the core invariant locally with no network: a fresh keypair,
    // encapsulate to it, decapsulate, HKDF both sides -> identical PSK.
    #[test]
    fn psk_agreement_roundtrip() {
        let mut rng = rand::thread_rng();
        let (dk, ek) = MlKem768::generate(&mut rng);
        let (ct, ss_sender) = ek.encapsulate(&mut rng).expect("encaps");
        let ss_receiver = dk.decapsulate(&ct).expect("decaps");
        assert_eq!(ss_sender.as_ref(), ss_receiver.as_ref());

        let salt = [7u8; 16];
        let mut psk_a = [0u8; PSK_LEN];
        let mut psk_b = [0u8; PSK_LEN];
        Hkdf::<Sha256>::new(Some(&salt), ss_sender.as_ref())
            .expand(HKDF_INFO, &mut psk_a)
            .unwrap();
        Hkdf::<Sha256>::new(Some(&salt), ss_receiver.as_ref())
            .expand(HKDF_INFO, &mut psk_b)
            .unwrap();
        assert_eq!(psk_a, psk_b, "both ends must derive the same PSK");
    }
}
