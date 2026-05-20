// Package peers defines peer data types.
package peers

import (
	"crypto/sha256"
	"encoding/hex"
	"time"
)

// Peer represents a connected VPN peer in the database.
type Peer struct {
	// WGPubkey is the peer's WireGuard public key (base64-encoded).
	// This serves as the primary key.
	WGPubkey string

	// AllowedIP is the VPN-internal IP address allocated to this peer.
	AllowedIP string

	// PSKHash is the SHA-256 hash of the derived PSK (hex-encoded).
	// We never store the raw PSK.
	PSKHash string

	// ConnectedAt is the timestamp when the peer connected.
	ConnectedAt time.Time

	// LastSeen is the timestamp of the peer's last activity.
	LastSeen time.Time
}

// HashPSK computes a SHA-256 hash of the PSK bytes and returns it hex-encoded.
// This is used to store a fingerprint of the PSK without retaining the secret.
func HashPSK(psk []byte) string {
	h := sha256.Sum256(psk)
	return hex.EncodeToString(h[:])
}
