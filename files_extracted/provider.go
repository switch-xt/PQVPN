// server/internal/psk/provider.go
//
// Server side of the PSK negotiation. Mirrors client/.../pqc/psk_provider.rs
// exactly: same wire format, same HKDF inputs => same 32-byte PSK.
//
// Requires Go 1.24+ : ML-KEM-768 is in the standard library (crypto/mlkem).
// There is intentionally NO liboqs-go dependency. Do not add one.
//
//   go 1.24
//   require golang.org/x/crypto v0.x   // only for hkdf (or use stdlib crypto/hkdf on 1.24)
//
// The server authenticates the peer via auth_token (JWT for v1 — verify it
// before trusting wg_pubkey). The TLS layer (self-signed cert that the client
// PINS) provides channel integrity so the ciphertext cannot be swapped.

package psk

import (
	"bufio"
	"crypto/mlkem"
	"crypto/rand"
	"crypto/tls"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"net"
	"os/exec"

	"golang.org/x/crypto/hkdf"
	"crypto/sha256"
	"io"
)

const (
	PSKLen   = 32
	hkdfInfo = "pqvpn-psk-v1"
)

type clientHello struct {
	EKB64     string `json:"ek_b64"`
	WGPubkey  string `json:"wg_pubkey"`
	AuthToken string `json:"auth_token"`
}

type serverResponse struct {
	CTB64          string `json:"ct_b64"`
	ServerWGPubkey string `json:"server_wg_pubkey"`
	ServerEndpoint string `json:"server_endpoint"`
	HKDFSaltB64    string `json:"hkdf_salt_b64"`
}

// Config holds the static facts the server announces to clients.
type Config struct {
	ServerWGPubkey string // server's WireGuard public key
	ServerEndpoint string // e.g. "203.0.113.10:51820"
	WGInterface    string // e.g. "wg0"
	AllowedIP      string // e.g. "10.8.0.2/32" (assign per peer in real impl)
}

// AuthFunc validates the peer's token and returns nil if it is allowed.
// Wire your JWT verification here. Reject early; never wg-set an unauth peer.
type AuthFunc func(token, wgPubkey string) error

// HandleConn runs one negotiation on an already-accepted TLS connection.
// Returns the negotiated PSK (for logging/metrics; the WG peer is already set).
func HandleConn(conn net.Conn, cfg Config, auth AuthFunc) ([]byte, error) {
	defer conn.Close()
	r := bufio.NewReader(conn)

	rawHello, err := r.ReadBytes('\n')
	if err != nil {
		return nil, fmt.Errorf("read ClientHello: %w", err)
	}
	var hello clientHello
	if err := json.Unmarshal(rawHello, &hello); err != nil {
		return nil, fmt.Errorf("parse ClientHello: %w", err)
	}

	if auth != nil {
		if err := auth(hello.AuthToken, hello.WGPubkey); err != nil {
			return nil, fmt.Errorf("auth rejected: %w", err)
		}
	}

	ekBytes, err := base64.StdEncoding.DecodeString(hello.EKB64)
	if err != nil {
		return nil, fmt.Errorf("bad ek base64: %w", err)
	}
	ek, err := mlkem.NewEncapsulationKey768(ekBytes)
	if err != nil {
		return nil, fmt.Errorf("invalid ML-KEM-768 encapsulation key: %w", err)
	}

	// Encapsulate -> (sharedSecret, ciphertext). PQ secrecy originates here.
	sharedSecret, ciphertext := ek.Encapsulate()

	// Per-session salt (16 random bytes), echoed to client so both HKDF
	// identically.
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		return nil, fmt.Errorf("salt rng: %w", err)
	}

	psk, err := derivePSK(sharedSecret, salt)
	if err != nil {
		return nil, err
	}

	// Program the WireGuard peer with this PSK BEFORE telling the client,
	// so the data plane is ready the instant the client brings the tunnel up.
	if err := wgSetPeer(cfg.WGInterface, hello.WGPubkey, psk, cfg.AllowedIP); err != nil {
		return nil, fmt.Errorf("wg set peer: %w", err)
	}

	resp := serverResponse{
		CTB64:          base64.StdEncoding.EncodeToString(ciphertext),
		ServerWGPubkey: cfg.ServerWGPubkey,
		ServerEndpoint: cfg.ServerEndpoint,
		HKDFSaltB64:    base64.StdEncoding.EncodeToString(salt),
	}
	out, _ := json.Marshal(resp)
	out = append(out, '\n')
	if _, err := conn.Write(out); err != nil {
		return nil, fmt.Errorf("write ServerResponse: %w", err)
	}
	return psk, nil
}

func derivePSK(sharedSecret, salt []byte) ([]byte, error) {
	rd := hkdf.New(sha256.New, sharedSecret, salt, []byte(hkdfInfo))
	psk := make([]byte, PSKLen)
	if _, err := io.ReadFull(rd, psk); err != nil {
		return nil, fmt.Errorf("hkdf expand: %w", err)
	}
	return psk, nil
}

// wgSetPeer runs: wg set <if> peer <pub> preshared-key /dev/stdin allowed-ips <ip>
// Passing the PSK via stdin avoids it ever touching argv / process list.
func wgSetPeer(iface, peerPub string, psk []byte, allowedIP string) error {
	pskB64 := base64.StdEncoding.EncodeToString(psk)
	cmd := exec.Command("wg", "set", iface,
		"peer", peerPub,
		"preshared-key", "/dev/stdin",
		"allowed-ips", allowedIP,
	)
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return err
	}
	go func() {
		defer stdin.Close()
		fmt.Fprintln(stdin, pskB64)
	}()
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("wg set failed: %v: %s", err, out)
	}
	return nil
}

// Serve binds a pinned-cert TLS listener and handles negotiations forever.
// `cert` is the same self-signed cert whose DER the client has compiled in.
func Serve(addr string, cert tls.Certificate, cfg Config, auth AuthFunc) error {
	ln, err := tls.Listen("tcp", addr, &tls.Config{
		Certificates: []tls.Certificate{cert},
		MinVersion:   tls.VersionTLS13,
	})
	if err != nil {
		return err
	}
	for {
		c, err := ln.Accept()
		if err != nil {
			continue
		}
		go func() {
			if psk, err := HandleConn(c, cfg, auth); err != nil {
				fmt.Println("negotiation error:", err)
			} else {
				fmt.Printf("PSK negotiated (b64 prefix): %s...\n",
					base64.StdEncoding.EncodeToString(psk)[:12])
			}
		}()
	}
}
