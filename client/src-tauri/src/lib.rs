//! PQVPN Tauri client backend.
//!
//! This crate provides the Rust backend for the PQVPN desktop client,
//! featuring ML-KEM-768 post-quantum key exchange and WireGuard tunnel management.

mod commands;
mod config;
mod dns;
mod killswitch;
mod pqc;
pub mod tunnel;
mod wireguard;

use commands::AppState;
use config::AppConfig;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_config = AppConfig::load();
    let _ = app_config.save(); // Save the default config to disk immediately if it didn't exist

    let app_state = AppState {
        connection: Mutex::new(commands::ConnectionState::default()),
        config: Mutex::new(app_config),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::connect,
            commands::disconnect,
            commands::get_status,
            dns::set_dns_protection,
            killswitch::toggle_kill_switch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
