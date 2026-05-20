// Package wg provides a WireGuard CLI wrapper for managing peers.
//
// All PSK values are passed to `wg` via stdin to avoid leaking secrets
// through command-line arguments visible in /proc.
package wg

import (
	"bytes"
	"fmt"
	"os/exec"
	"strings"
)

// Manager wraps the WireGuard CLI tools for a specific interface.
type Manager struct {
	iface string
}

// NewManager creates a new WireGuard manager for the given interface.
func NewManager(iface string) *Manager {
	return &Manager{iface: iface}
}

// SetPeer configures a WireGuard peer on the managed interface.
//
// The PSK is passed via stdin to avoid exposing it in process arguments.
// This runs: wg set <iface> peer <pubkey> preshared-key /dev/stdin allowed-ips <allowedIP>
func (m *Manager) SetPeer(peerPubkey, pskBase64, allowedIP string) error {
	// Build the wg set command.
	// preshared-key reads from /dev/stdin so the PSK is never in argv.
	cmd := exec.Command("wg", "set", m.iface,
		"peer", peerPubkey,
		"preshared-key", "/dev/stdin",
		"allowed-ips", allowedIP,
	)

	// Feed the PSK via stdin.
	cmd.Stdin = strings.NewReader(pskBase64 + "\n")

	var stderr bytes.Buffer
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("wg set peer failed: %w (stderr: %s)", err, stderr.String())
	}

	return nil
}

// RemovePeer removes a WireGuard peer from the managed interface.
//
// This runs: wg set <iface> peer <pubkey> remove
func (m *Manager) RemovePeer(peerPubkey string) error {
	cmd := exec.Command("wg", "set", m.iface,
		"peer", peerPubkey,
		"remove",
	)

	var stderr bytes.Buffer
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return fmt.Errorf("wg set peer remove failed: %w (stderr: %s)", err, stderr.String())
	}

	return nil
}

// PeerStats holds statistics for a single WireGuard peer.
type PeerStats struct {
	PublicKey       string
	Endpoint        string
	AllowedIPs      string
	LatestHandshake string
	TransferRx      string
	TransferTx      string
}

// GetPeerStats retrieves statistics for all peers on the managed interface
// by parsing the output of `wg show <iface>`.
func (m *Manager) GetPeerStats() ([]PeerStats, error) {
	cmd := exec.Command("wg", "show", m.iface, "dump")

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("wg show failed: %w (stderr: %s)", err, stderr.String())
	}

	return parseDump(stdout.String()), nil
}

// parseDump parses the tab-separated output of `wg show <iface> dump`.
//
// The first line is the interface line (private-key, listen-port, fwmark).
// Subsequent lines are peer lines with fields:
//   public-key, preshared-key, endpoint, allowed-ips, latest-handshake, transfer-rx, transfer-tx, persistent-keepalive
func parseDump(output string) []PeerStats {
	var stats []PeerStats
	lines := strings.Split(strings.TrimSpace(output), "\n")

	// Skip the first line (interface info).
	for i := 1; i < len(lines); i++ {
		fields := strings.Split(lines[i], "\t")
		if len(fields) < 7 {
			continue
		}

		stats = append(stats, PeerStats{
			PublicKey:       fields[0],
			Endpoint:        fields[2],
			AllowedIPs:      fields[3],
			LatestHandshake: fields[4],
			TransferRx:      fields[5],
			TransferTx:      fields[6],
		})
	}

	return stats
}
