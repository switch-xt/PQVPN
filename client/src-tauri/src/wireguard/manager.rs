use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tauri::{AppHandle, Emitter};

pub struct TunnelMonitor {
    tunnel_name: String,
    app_handle: AppHandle,
}

impl TunnelMonitor {
    pub fn new(tunnel_name: String, app_handle: AppHandle) -> Self {
        Self { tunnel_name, app_handle }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            // Wait 15 seconds after connect before starting to monitor
            sleep(Duration::from_secs(15)).await;

            let mut consecutive_fails = 0u32;

            loop {
                // Check every 30 seconds instead of 10 — much less aggressive
                sleep(Duration::from_secs(30)).await;

                let is_up = crate::wireguard::windows::tunnel_is_up(&self.tunnel_name);

                if !is_up {
                    consecutive_fails += 1;

                    // Only emit "failed" after 3 consecutive failures (90 seconds of downtime)
                    // This avoids false positives from brief service restarts
                    if consecutive_fails >= 3 {
                        let _ = self.app_handle.emit(
                            "tunnel_state_changed",
                            serde_json::json!({ "state": "failed" }),
                        );
                        // Stop monitoring — the user must manually reconnect
                        break;
                    }
                } else {
                    consecutive_fails = 0;
                }
            }
        });
    }
}
