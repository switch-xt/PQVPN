// Package psk implements ML-KEM-768 post-quantum PSK negotiation.
//
// Wire format uses newline-delimited JSON over TLS:
//   - Client sends ClientHello: {ek_b64, wg_pubkey, auth_token}
//   - Server responds ServerResponse: {ct_b64, server_wg_pubkey, server_endpoint, hkdf_salt_b64}
//
// The shared secret from ML-KEM encapsulation is run through HKDF-SHA256
// with info string "pqvpn-psk-v1" to derive a 32-byte WireGuard PSK.
package psk

import (
	"crypto/mlkem"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"fmt"
	"io"

	"golang.org/x/crypto/hkdf"
)

const (
	// HKDFInfo is the info string used in HKDF derivation.
	// Must match the client implementation exactly.
	HKDFInfo = "pqvpn-psk-v1"

	// PSKLength is the length of the derived WireGuard PSK in bytes.
	PSKLength = 32

	// SaltLength is the length of the random HKDF salt in bytes.
	SaltLength = 32
)

// ClientHello is the initial message sent by a connecting client.
type ClientHello struct {
	// EKBase64 is the ML-KEM-768 encapsulation key, base64-encoded.
	EKBase64 string `json:"ek_b64"`

	// WGPubkey is the client's WireGuard public key (base64).
	WGPubkey string `json:"wg_pubkey"`

	// AuthToken is an optional authentication token.
	AuthToken string `json:"auth_token,omitempty"`
}

// ServerResponse is the server's reply to a ClientHello.
type ServerResponse struct {
	// CTBase64 is the ML-KEM-768 ciphertext, base64-encoded.
	CTBase64 string `json:"ct_b64"`

	// ServerWGPubkey is the server's WireGuard public key (base64).
	ServerWGPubkey string `json:"server_wg_pubkey"`

	// ServerEndpoint is the server's WireGuard endpoint (host:port).
	ServerEndpoint string `json:"server_endpoint"`

	// HKDFSaltBase64 is the random HKDF salt, base64-encoded.
	HKDFSaltBase64 string `json:"hkdf_salt_b64"`

	// AllowedIP is the IP address allocated to the client within the VPN.
	AllowedIP string `json:"allowed_ip"`
}

// ErrorResponse is returned when the server encounters an error.
type ErrorResponse struct {
	Error string `json:"error"`
}

// NegotiateResult holds the outputs of a successful PSK negotiation.
type NegotiateResult struct {
	// PSK is the 32-byte derived pre-shared key for WireGuard.
	PSK [PSKLength]byte

	// Ciphertext is the ML-KEM ciphertext to send back to the client.
	Ciphertext []byte

	// Salt is the HKDF salt used in key derivation.
	Salt []byte
}

// Negotiate performs the server side of an ML-KEM-768 PSK exchange.
//
// It takes the client's encapsulation key (raw bytes), performs encapsulation
// to get a shared secret and ciphertext, then derives a 32-byte PSK using
// HKDF-SHA256.
func Negotiate(ekBytes []byte) (*NegotiateResult, error) {
	// Parse the client's ML-KEM-768 encapsulation key.
	ek, err := mlkem.NewEncapsulationKey768(ekBytes)
	if err != nil {
		return nil, fmt.Errorf("invalid ML-KEM-768 encapsulation key: %w", err)
	}

	// Encapsulate: produces a shared secret and ciphertext.
	// The shared secret is known only to us; the client can recover it
	// using their decapsulation key and the ciphertext.
	sharedSecret, ciphertext := ek.Encapsulate()

	// Generate a random salt for HKDF.
	salt := make([]byte, SaltLength)
	if _, err := io.ReadFull(rand.Reader, salt); err != nil {
		return nil, fmt.Errorf("failed to generate HKDF salt: %w", err)
	}

	// Derive the PSK using HKDF-SHA256.
	psk, err := derivePSK(sharedSecret, salt)
	if err != nil {
		return nil, fmt.Errorf("HKDF derivation failed: %w", err)
	}

	return &NegotiateResult{
		PSK:        psk,
		Ciphertext: ciphertext,
		Salt:       salt,
	}, nil
}

// derivePSK derives a 32-byte PSK from a shared secret using HKDF-SHA256.
func derivePSK(sharedSecret, salt []byte) ([PSKLength]byte, error) {
	var psk [PSKLength]byte

	hkdfReader := hkdf.New(sha256.New, sharedSecret, salt, []byte(HKDFInfo))
	if _, err := io.ReadFull(hkdfReader, psk[:]); err != nil {
		return psk, fmt.Errorf("failed to read from HKDF: %w", err)
	}

	return psk, nil
}

// EncodeBase64 encodes raw bytes to standard base64 string.
func EncodeBase64(data []byte) string {
	return base64.StdEncoding.EncodeToString(data)
}

// DecodeBase64 decodes a standard base64 string to raw bytes.
func DecodeBase64(s string) ([]byte, error) {
	return base64.StdEncoding.DecodeString(s)
}
