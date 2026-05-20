package peers

import (
	"database/sql"
	"fmt"
	"net"
	"sync"
	"time"

	_ "modernc.org/sqlite"
)

// SQLiteStore manages peer data in a SQLite database.
type SQLiteStore struct {
	db *sql.DB
	mu sync.Mutex // protects IP allocation
}

// NewSQLiteStore opens (or creates) a SQLite database at the given path
// and ensures the schema is initialized.
func NewSQLiteStore(dbPath string) (*SQLiteStore, error) {
	db, err := sql.Open("sqlite", dbPath)
	if err != nil {
		return nil, fmt.Errorf("failed to open database: %w", err)
	}

	// Enable WAL mode for better concurrency.
	if _, err := db.Exec("PRAGMA journal_mode=WAL"); err != nil {
		db.Close()
		return nil, fmt.Errorf("failed to set WAL mode: %w", err)
	}

	// Create the peers table if it doesn't exist.
	createTable := `
	CREATE TABLE IF NOT EXISTS peers (
		wg_pubkey    TEXT PRIMARY KEY,
		allowed_ip   TEXT NOT NULL UNIQUE,
		psk_hash     TEXT NOT NULL,
		connected_at TEXT NOT NULL,
		last_seen    TEXT NOT NULL
	)`
	if _, err := db.Exec(createTable); err != nil {
		db.Close()
		return nil, fmt.Errorf("failed to create peers table: %w", err)
	}

	return &SQLiteStore{db: db}, nil
}

// Close closes the underlying database connection.
func (s *SQLiteStore) Close() error {
	return s.db.Close()
}

// AddPeer inserts a new peer into the database.
// If a peer with the same WG pubkey already exists, it is replaced.
func (s *SQLiteStore) AddPeer(p *Peer) error {
	now := time.Now().UTC().Format(time.RFC3339)
	_, err := s.db.Exec(
		`INSERT OR REPLACE INTO peers (wg_pubkey, allowed_ip, psk_hash, connected_at, last_seen)
		 VALUES (?, ?, ?, ?, ?)`,
		p.WGPubkey, p.AllowedIP, p.PSKHash, now, now,
	)
	if err != nil {
		return fmt.Errorf("failed to add peer: %w", err)
	}
	return nil
}

// RemovePeer deletes a peer by its WireGuard public key.
func (s *SQLiteStore) RemovePeer(wgPubkey string) error {
	result, err := s.db.Exec("DELETE FROM peers WHERE wg_pubkey = ?", wgPubkey)
	if err != nil {
		return fmt.Errorf("failed to remove peer: %w", err)
	}
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return fmt.Errorf("peer not found: %s", wgPubkey)
	}
	return nil
}

// GetPeer retrieves a peer by its WireGuard public key.
func (s *SQLiteStore) GetPeer(wgPubkey string) (*Peer, error) {
	row := s.db.QueryRow(
		"SELECT wg_pubkey, allowed_ip, psk_hash, connected_at, last_seen FROM peers WHERE wg_pubkey = ?",
		wgPubkey,
	)

	p := &Peer{}
	var connectedAt, lastSeen string
	if err := row.Scan(&p.WGPubkey, &p.AllowedIP, &p.PSKHash, &connectedAt, &lastSeen); err != nil {
		if err == sql.ErrNoRows {
			return nil, fmt.Errorf("peer not found: %s", wgPubkey)
		}
		return nil, fmt.Errorf("failed to get peer: %w", err)
	}

	p.ConnectedAt, _ = time.Parse(time.RFC3339, connectedAt)
	p.LastSeen, _ = time.Parse(time.RFC3339, lastSeen)
	return p, nil
}

// ListPeers returns all peers in the database.
func (s *SQLiteStore) ListPeers() ([]*Peer, error) {
	rows, err := s.db.Query(
		"SELECT wg_pubkey, allowed_ip, psk_hash, connected_at, last_seen FROM peers ORDER BY connected_at",
	)
	if err != nil {
		return nil, fmt.Errorf("failed to list peers: %w", err)
	}
	defer rows.Close()

	var peers []*Peer
	for rows.Next() {
		p := &Peer{}
		var connectedAt, lastSeen string
		if err := rows.Scan(&p.WGPubkey, &p.AllowedIP, &p.PSKHash, &connectedAt, &lastSeen); err != nil {
			return nil, fmt.Errorf("failed to scan peer row: %w", err)
		}
		p.ConnectedAt, _ = time.Parse(time.RFC3339, connectedAt)
		p.LastSeen, _ = time.Parse(time.RFC3339, lastSeen)
		peers = append(peers, p)
	}

	return peers, rows.Err()
}

// AllocateIP finds the next available IP in the 10.8.0.0/24 subnet.
// The server uses 10.8.0.1, so peers start at 10.8.0.2.
// This is a simple incrementing allocator that checks existing assignments.
func (s *SQLiteStore) AllocateIP() (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Collect all currently allocated IPs.
	rows, err := s.db.Query("SELECT allowed_ip FROM peers")
	if err != nil {
		return "", fmt.Errorf("failed to query allocated IPs: %w", err)
	}
	defer rows.Close()

	allocated := make(map[string]bool)
	for rows.Next() {
		var ip string
		if err := rows.Scan(&ip); err != nil {
			return "", fmt.Errorf("failed to scan IP: %w", err)
		}
		allocated[ip] = true
	}
	if err := rows.Err(); err != nil {
		return "", fmt.Errorf("error iterating IPs: %w", err)
	}

	// Start from 10.8.0.2 and find the first unallocated IP.
	// Support up to 10.8.255.254 (a /16 range gives us ~65k peers).
	baseIP := net.IPv4(10, 8, 0, 2)
	for i := 0; i < 65533; i++ {
		candidate := incrementIP(baseIP, i)
		candidateStr := candidate.String()
		if !allocated[candidateStr] {
			return candidateStr, nil
		}
	}

	return "", fmt.Errorf("no available IP addresses in the VPN subnet")
}

// UpdateLastSeen updates the last_seen timestamp for a peer.
func (s *SQLiteStore) UpdateLastSeen(wgPubkey string) error {
	now := time.Now().UTC().Format(time.RFC3339)
	_, err := s.db.Exec("UPDATE peers SET last_seen = ? WHERE wg_pubkey = ?", now, wgPubkey)
	if err != nil {
		return fmt.Errorf("failed to update last_seen: %w", err)
	}
	return nil
}

// incrementIP adds an offset to a base IPv4 address.
func incrementIP(base net.IP, offset int) net.IP {
	ip := make(net.IP, len(base))
	copy(ip, base)

	// Convert last two octets to a 16-bit value, add offset, write back.
	val := int(ip[14])<<8 | int(ip[15])
	val += offset

	ip[14] = byte(val >> 8)
	ip[15] = byte(val & 0xFF)

	// Handle overflow into the third octet.
	if ip[15] == 0 || ip[15] == 255 {
		// Skip .0 (network) and .255 (broadcast) addresses.
		// Just bump by one more.
		return incrementIP(base, offset+1)
	}

	return ip
}
