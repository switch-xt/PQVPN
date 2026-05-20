use std::process::Command;
use anyhow::{Result, anyhow, Context};
use winreg::enums::*;
use winreg::RegKey;

/// Toggle the system-wide kill switch.
/// 
/// When enabled, all non-tunnel, non-loopback outbound traffic is blocked.
#[tauri::command]
pub async fn toggle_kill_switch(enabled: bool) -> Result<String, String> {
    if enabled {
        enable_killswitch().map_err(|e| format!("Failed to enable kill switch: {}", e))?;
        set_registry_state(true).map_err(|e| format!("Registry error: {}", e))?;
        Ok("Kill switch enabled".to_string())
    } else {
        disable_killswitch().map_err(|e| format!("Failed to disable kill switch: {}", e))?;
        set_registry_state(false).map_err(|e| format!("Registry error: {}", e))?;
        Ok("Kill switch disabled".to_string())
    }
}

fn set_registry_state(enabled: bool) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\PQVPN")?;
    let val: u32 = if enabled { 1 } else { 0 };
    key.set_value("KillSwitchEnabled", &val)?;
    Ok(())
}

fn enable_killswitch() -> Result<()> {
    let _ = disable_killswitch(); // Clean up existing rules

    // Block all outbound traffic
    let script_block = "New-NetFirewallRule -DisplayName 'PQVPN_KillSwitch_BlockAll' -Direction Outbound -Action Block -Profile Any";
    // Allow localhost
    let script_allow_local = "New-NetFirewallRule -DisplayName 'PQVPN_KillSwitch_AllowLocal' -Direction Outbound -Action Allow -RemoteAddress '127.0.0.1','::1' -Profile Any";
    // Allow tunnel subnet (10.8.0.0/24)
    let script_allow_wg = "New-NetFirewallRule -DisplayName 'PQVPN_KillSwitch_AllowWG' -Direction Outbound -Action Allow -RemoteAddress '10.8.0.0/24' -Profile Any";
    
    // Also allow traffic to the GCP server on port 443 so the tunnel itself doesn't get blocked
    let script_allow_server = "New-NetFirewallRule -DisplayName 'PQVPN_KillSwitch_AllowServer' -Direction Outbound -Action Allow -RemoteAddress '34.136.62.117' -RemotePort 443 -Protocol TCP -Profile Any";

    let combined = format!("{}; {}; {}; {}", script_block, script_allow_local, script_allow_wg, script_allow_server);

    let output = Command::new("powershell")
        .args(["-Command", &format!("Start-Process powershell -ArgumentList '-WindowStyle Hidden -Command \"{}\"' -Verb RunAs -Wait", combined)])
        .output()
        .context("Failed to execute PowerShell for Kill Switch")?;

    if !output.status.success() {
        return Err(anyhow!("Kill switch enable failed"));
    }

    Ok(())
}

fn disable_killswitch() -> Result<()> {
    let script = "Remove-NetFirewallRule -DisplayName 'PQVPN_KillSwitch_*' -ErrorAction SilentlyContinue";

    let output = Command::new("powershell")
        .args(["-Command", &format!("Start-Process powershell -ArgumentList '-WindowStyle Hidden -Command \"{}\"' -Verb RunAs -Wait", script)])
        .output()
        .context("Failed to execute PowerShell for Kill Switch removal")?;

    if !output.status.success() {
        return Err(anyhow!("Kill switch disable failed"));
    }

    Ok(())
}
