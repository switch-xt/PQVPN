package api

import (
	"encoding/json"
	"log"
	"net/http"
	"os/exec"
	"sync"

	"github.com/pqvpn/server/internal/peers"
	"github.com/pqvpn/server/internal/psk"
	"github.com/pqvpn/server/internal/wg"
)

var shareMap sync.Map

// Handler implements the HTTP route handlers for the pqvpnd API.
type Handler struct {
	store          *peers.SQLiteStore
	wgMgr          *wg.Manager
	serverWGPubkey string
	serverEndpoint string
}

// DisconnectRequest is the JSON body for the disconnect endpoint.
type DisconnectRequest struct {
	WGPubkey string `json:"wg_pubkey"`
}

// StatusResponse is the JSON body returned by the status endpoint.
type StatusResponse struct {
	Status    string `json:"status"`
	PeerCount int    `json:"peer_count"`
	Version   string `json:"version"`
}

// HandleConnect handles POST /api/connect.
//
// Flow:
//  1. Parse ClientHello (ML-KEM encapsulation key + WG pubkey)
//  2. Run ML-KEM-768 encapsulation → shared secret + ciphertext
//  3. Derive 32-byte PSK via HKDF-SHA256
//  4. Allocate an IP for the peer
//  5. Register the peer in WireGuard via `wg set`
//  6. Store peer info in SQLite
//  7. Return ServerResponse (ciphertext + server WG info)
func (h *Handler) HandleConnect(w http.ResponseWriter, r *http.Request) {
	// Parse the client's hello message.
	var hello psk.ClientHello
	if err := json.NewDecoder(r.Body).Decode(&hello); err != nil {
		writeError(w, http.StatusBadRequest, "invalid request body: "+err.Error())
		return
	}

	if hello.EKBase64 == "" {
		writeError(w, http.StatusBadRequest, "missing encapsulation key (ek_b64)")
		return
	}
	if hello.WGPubkey == "" {
		writeError(w, http.StatusBadRequest, "missing WireGuard public key (wg_pubkey)")
		return
	}

	// Decode the ML-KEM encapsulation key.
	ekBytes, err := psk.DecodeBase64(hello.EKBase64)
	if err != nil {
		writeError(w, http.StatusBadRequest, "invalid base64 encapsulation key: "+err.Error())
		return
	}

	// Perform the ML-KEM-768 PSK negotiation.
	result, err := psk.Negotiate(ekBytes)
	if err != nil {
		log.Printf("PSK negotiation failed for peer %s: %v", hello.WGPubkey, err)
		writeError(w, http.StatusInternalServerError, "PSK negotiation failed")
		return
	}

	// Allocate a VPN IP address for the peer.
	allocatedIP, err := h.store.AllocateIP()
	if err != nil {
		log.Printf("IP allocation failed: %v", err)
		writeError(w, http.StatusInternalServerError, "failed to allocate IP address")
		return
	}

	// Configure the WireGuard peer with the derived PSK.
	pskB64 := psk.EncodeBase64(result.PSK[:])
	allowedIPs := fmt.Sprintf("%s/32", allocatedIP)
	if hello.ShareCode != "" {
		allowedIPs += ", 0.0.0.0/0"
	}
	if err := h.wgMgr.SetPeer(hello.WGPubkey, pskB64, allowedIPs); err != nil {
		log.Printf("Failed to set WireGuard peer %s: %v", hello.WGPubkey, err)
		writeError(w, http.StatusInternalServerError, "failed to configure WireGuard peer")
		return
	}

	// Compute a hash of the PSK for storage (we never store the raw PSK).
	pskHash := peers.HashPSK(result.PSK[:])

	// Store peer information.
	peer := &peers.Peer{
		WGPubkey:  hello.WGPubkey,
		AllowedIP: allocatedIP,
		PSKHash:   pskHash,
	}
	if err := h.store.AddPeer(peer); err != nil {
		log.Printf("Failed to store peer %s: %v", hello.WGPubkey, err)
		// Peer is already configured in WG, but DB storage failed.
		// Try to clean up the WG peer.
		_ = h.wgMgr.RemovePeer(hello.WGPubkey)
		writeError(w, http.StatusInternalServerError, "failed to store peer information")
		return
	}

	log.Printf("Peer connected: %s → %s", hello.WGPubkey, allocatedIP)
	if hello.Mode != "" {
		log.Printf("  Connection mode: %s", hello.Mode)
	}
	if hello.Mode == "peer" {
		log.Printf("  Peer relay: enabling client-to-client routing for %s", allocatedIP)
	}

	if hello.ShareCode != "" {
		shareMap.Store(hello.ShareCode, allocatedIP)
		log.Printf("Peer is sharing internet with code %s", hello.ShareCode)
	}

	if hello.TargetCode != "" {
		if val, ok := shareMap.Load(hello.TargetCode); ok {
			sharerIP := val.(string)
			log.Printf("Routing peer %s traffic through sharer %s (code: %s)", allocatedIP, sharerIP, hello.TargetCode)

			cmd1 := exec.Command("ip", "rule", "add", "from", allocatedIP, "table", "100")
			if err := cmd1.Run(); err != nil {
				log.Printf("Note: failed to add ip rule (might already exist): %v", err)
			}

			cmd2 := exec.Command("ip", "route", "add", "default", "via", sharerIP, "dev", "wg0", "table", "100")
			if err := cmd2.Run(); err != nil {
				log.Printf("Note: failed to add ip route (might already exist): %v", err)
			}
		} else {
			log.Printf("Warning: TargetCode %s not found in shareMap", hello.TargetCode)
		}
	}

	// Build and send the server response.
	resp := psk.ServerResponse{
		CTBase64:       psk.EncodeBase64(result.Ciphertext),
		ServerWGPubkey: h.serverWGPubkey,
		ServerEndpoint: h.serverEndpoint,
		HKDFSaltBase64: psk.EncodeBase64(result.Salt),
		AllowedIP:      allocatedIP,
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(resp)
}

// HandleDisconnect handles DELETE /api/disconnect.
//
// Removes a peer from both WireGuard and the peer store.
func (h *Handler) HandleDisconnect(w http.ResponseWriter, r *http.Request) {
	var req DisconnectRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, "invalid request body: "+err.Error())
		return
	}

	if req.WGPubkey == "" {
		writeError(w, http.StatusBadRequest, "missing WireGuard public key (wg_pubkey)")
		return
	}

	// Verify the peer exists.
	_, err := h.store.GetPeer(req.WGPubkey)
	if err != nil {
		writeError(w, http.StatusNotFound, "peer not found")
		return
	}

	// Remove from WireGuard.
	if err := h.wgMgr.RemovePeer(req.WGPubkey); err != nil {
		log.Printf("Failed to remove WireGuard peer %s: %v", req.WGPubkey, err)
		writeError(w, http.StatusInternalServerError, "failed to remove WireGuard peer")
		return
	}

	// Remove from the store.
	if err := h.store.RemovePeer(req.WGPubkey); err != nil {
		log.Printf("Failed to remove peer %s from store: %v", req.WGPubkey, err)
		writeError(w, http.StatusInternalServerError, "failed to remove peer from store")
		return
	}

	log.Printf("Peer disconnected: %s", req.WGPubkey)

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(map[string]string{"status": "disconnected"})
}

// HandleStatus handles GET /api/status.
//
// Returns server health and the current number of connected peers.
func (h *Handler) HandleStatus(w http.ResponseWriter, r *http.Request) {
	peerList, err := h.store.ListPeers()
	if err != nil {
		log.Printf("Failed to list peers: %v", err)
		writeError(w, http.StatusInternalServerError, "failed to query peer count")
		return
	}

	resp := StatusResponse{
		Status:    "ok",
		PeerCount: len(peerList),
		Version:   "0.1.0",
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)
	json.NewEncoder(w).Encode(resp)
}

// writeError sends a JSON error response.
func writeError(w http.ResponseWriter, statusCode int, message string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(statusCode)
	json.NewEncoder(w).Encode(psk.ErrorResponse{Error: message})
}
