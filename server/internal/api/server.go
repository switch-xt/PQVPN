// Package api implements the HTTPS API server for pqvpnd.
package api

import (
	"crypto/tls"
	"net/http"

	"github.com/pqvpn/server/internal/peers"
	"github.com/pqvpn/server/internal/wg"
)

// Config holds configuration for the API server.
type Config struct {
	ListenAddr     string
	TLSConfig      *tls.Config
	Store          *peers.SQLiteStore
	WGManager      *wg.Manager
	ServerWGPubkey string
	ServerEndpoint string
}

// Server is the pqvpnd API server.
type Server struct {
	httpServer *http.Server
	handler    *Handler
}

// NewServer creates a new API server with the given configuration.
func NewServer(cfg Config) *Server {
	h := &Handler{
		store:          cfg.Store,
		wgMgr:          cfg.WGManager,
		serverWGPubkey: cfg.ServerWGPubkey,
		serverEndpoint: cfg.ServerEndpoint,
	}

	mux := http.NewServeMux()
	mux.HandleFunc("POST /api/connect", h.HandleConnect)
	mux.HandleFunc("DELETE /api/disconnect", h.HandleDisconnect)
	mux.HandleFunc("GET /api/status", h.HandleStatus)

	return &Server{
		httpServer: &http.Server{
			Addr:      cfg.ListenAddr,
			Handler:   RateLimitMiddleware(mux),
			TLSConfig: cfg.TLSConfig,
		},
		handler: h,
	}
}

// ListenAndServeTLS starts the TLS server.
// The cert and key are already configured via TLSConfig, so we pass empty
// strings to let the standard library use the config directly.
func (s *Server) ListenAndServeTLS() error {
	return s.httpServer.ListenAndServeTLS("", "")
}
