package main

import (
	"crypto/tls"
	"flag"
	"fmt"
	"log"
	"os"
	"strings"

	"github.com/pqvpn/server/internal/api"
	"github.com/pqvpn/server/internal/peers"
	"github.com/pqvpn/server/internal/wg"
)

func main() {
	listenAddr := flag.String("listen", ":8443", "TLS listen address")
	wgIface := flag.String("wg-interface", "wg0", "WireGuard interface name")
	wgPubkey := flag.String("wg-pubkey", "", "Server WireGuard public key (base64)")
	wgPubkeyFile := flag.String("wg-pubkey-file", "", "Path to file containing server WireGuard public key")
	endpoint := flag.String("endpoint", "", "Server WireGuard endpoint (host:port)")
	certFile := flag.String("cert", "certs/server.crt", "TLS certificate file")
	keyFile := flag.String("key", "certs/server.key", "TLS private key file")
	dbPath := flag.String("db", "peers.db", "SQLite database path")
	flag.Parse()

	// Resolve WireGuard public key
	serverWGPubkey := *wgPubkey
	if serverWGPubkey == "" && *wgPubkeyFile != "" {
		data, err := os.ReadFile(*wgPubkeyFile)
		if err != nil {
			log.Fatalf("Failed to read WireGuard public key file: %v", err)
		}
		serverWGPubkey = strings.TrimSpace(string(data))
	}
	if serverWGPubkey == "" {
		log.Fatal("Server WireGuard public key is required (--wg-pubkey or --wg-pubkey-file)")
	}
	if *endpoint == "" {
		log.Fatal("Server endpoint is required (--endpoint)")
	}

	// Load TLS certificate
	cert, err := tls.LoadX509KeyPair(*certFile, *keyFile)
	if err != nil {
		log.Fatalf("Failed to load TLS certificate: %v", err)
	}

	tlsConfig := &tls.Config{
		Certificates: []tls.Certificate{cert},
		MinVersion:   tls.VersionTLS13,
	}

	// Initialize SQLite peer store
	store, err := peers.NewSQLiteStore(*dbPath)
	if err != nil {
		log.Fatalf("Failed to initialize peer store: %v", err)
	}
	defer store.Close()

	// Initialize WireGuard manager
	wgMgr := wg.NewManager(*wgIface)

	// Build and start the API server
	srv := api.NewServer(api.Config{
		ListenAddr:     *listenAddr,
		TLSConfig:      tlsConfig,
		Store:          store,
		WGManager:      wgMgr,
		ServerWGPubkey: serverWGPubkey,
		ServerEndpoint: *endpoint,
	})

	log.Printf("pqvpnd starting on %s", *listenAddr)
	log.Printf("  WireGuard interface: %s", *wgIface)
	log.Printf("  WireGuard endpoint:  %s", *endpoint)
	log.Printf("  Database:            %s", *dbPath)

	fmt.Println()
	log.Println("Server is ready to accept connections")

	if err := srv.ListenAndServeTLS(); err != nil {
		log.Fatalf("Server failed: %v", err)
	}
}
