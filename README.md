<div align="center">
  
# 🛡️ PQVPN (Quantum Shield)

**Next-Generation Post-Quantum VPN built with Rust, Go, and Tauri.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Tauri](https://img.shields.io/badge/Tauri-App-FFC131?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-Backend-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Go](https://img.shields.io/badge/Go-Relay_Server-00ADD8?logo=go&logoColor=white)](https://go.dev/)
[![WireGuard](https://img.shields.io/badge/Protocol-WireGuard-881798?logo=wireguard&logoColor=white)](https://www.wireguard.com/)

*Secure today's data against tomorrow's quantum computers.*

</div>

---

## 🌌 Overview

PQVPN is a state-of-the-art Virtual Private Network client engineered to defend against "Store Now, Decrypt Later" (SNDL) attacks. By combining the proven speed of **WireGuard** with cutting-edge **ML-KEM-768** (FIPS 203) post-quantum cryptography, PQVPN ensures your traffic remains completely impenetrable to both classical and quantum adversaries. 

Wrapped in a stunning, hardware-accelerated **Glassmorphism UI**, PQVPN proves that military-grade security doesn't have to look like a terminal.

## ✨ Core Features

* 🔐 **Quantum-Safe Key Exchange**: Utilizes Kyber/ML-KEM-768 for post-quantum secure Pre-Shared Keys (PSKs) injected directly into the WireGuard handshake.
* ⚡ **WireGuard Backbone**: Built on top of the extremely fast, in-kernel WireGuard protocol (ChaCha20-Poly1305).
* 🎨 **Premium UI/UX**: A dark-mode, glassmorphism interface built with React & Tauri, featuring real-time sparkline traffic graphs and dynamic micro-animations.
* 🕹️ **Gaming Mode**: Intelligent UDP packet optimization designed to minimize jitter and combat DDoS attacks for latency-sensitive applications.
* 🔀 **Peer Relay Network**: Multi-hop routing architecture designed to obscure traffic origin and defeat advanced traffic correlation analysis.
* 🛡️ **Instant Kill-Switch**: Native Windows Firewall integration instantly drops packets if the quantum tunnel drops, preventing IP leakage.

## 🏗️ Architecture

PQVPN leverages a tri-language architecture to achieve maximum performance and security:

1. **Frontend (React/TypeScript)**: Lightweight, reactive dashboard rendering live network metrics and cryptographic states.
2. **Client Backend (Rust/Tauri)**: Low-level system integration, automated Windows adapter management, firewall rule injection, and ML-KEM cryptographic execution.
3. **Server Backend (Go)**: A highly concurrent daemon that dynamically orchestrates WireGuard peers, negotiates quantum keys, and manages IP allocation.

## 🚀 Getting Started

### Prerequisites
* **Windows 10/11** (Admin privileges required for network adapter creation)
* **WireGuard for Windows** installed (PQVPN will automatically configure it)
* **Node.js** (v18+) & **Rust** (for building from source)

### Building the Client

```bash
# Clone the repository
git clone https://github.com/switch-xt/PQVPN.git
cd PQVPN/client

# Install frontend dependencies
npm install

# Build the executable (Tauri will compile the Rust backend)
npm run tauri build
```
The compiled executable will be located in `src-tauri/target/release/client.exe`.

### Running the Server

```bash
cd server
go mod download

# Build the Go daemon
go build -o pqvpnd ./cmd/pqvpnd

# Run the server daemon
sudo ./pqvpnd
```

## 🔒 Cryptographic Details

PQVPN employs a hybrid approach to maintain compliance and maximum security:
* **Transport**: WireGuard (Curve25519, ChaCha20, Poly1305, BLAKE2s)
* **Post-Quantum Layer**: ML-KEM-768 (Kyber)
* **Key Rotation**: Cryptographic PSKs are negotiated via a secure TLS 1.3 channel to the server daemon, established, and securely injected into the WireGuard interface prior to tunnel initialization.

## 📜 License
This project is licensed under the MIT License - see the LICENSE file for details.

---
<div align="center">
  <i>Built for privacy. Built for the future.</i>
</div>
