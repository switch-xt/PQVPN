use std::process::Command;
use anyhow::{Result, anyhow, Context};

/// Enable or disable NRPT DNS leak protection.
/// 
/// When enabled, all DNS queries are forced to the specified nameservers.
#[tauri::command]
pub async fn set_dns_protection(enabled: bool, nameservers: String) -> Result<String, String> {
    if enabled {
        enable_nrpt(&nameservers).map_err(|e| format!("Failed to enable DNS protection: {}", e))?;
        Ok("DNS protection enabled".to_string())
    } else {
        disable_nrpt().map_err(|e| format!("Failed to disable DNS protection: {}", e))?;
        Ok("DNS protection disabled".to_string())
    }
}

fn enable_nrpt(nameservers: &str) -> Result<()> {
    // First remove any existing rule to avoid duplicates
    let _ = disable_nrpt();

    // The namespace "." means all domains.
    let script = format!(
        "Add-DnsClientNrptRule -Namespace '.' -NameServers '{}' -Comment 'PQVPN'",
        nameservers
    );

    let output = Command::new("powershell")
        .args(["-Command", &format!("Start-Process powershell -ArgumentList '-WindowStyle Hidden -Command \"{}\"' -Verb RunAs -Wait", script)])
        .output()
        .context("Failed to execute PowerShell for NRPT rule")?;

    if !output.status.success() {
        return Err(anyhow!("NRPT rule addition failed"));
    }

    Ok(())
}

fn disable_nrpt() -> Result<()> {
    let script = "Get-DnsClientNrptRule | Where-Object {$_.Comment -eq 'PQVPN'} | Remove-DnsClientNrptRule -Force";

    let output = Command::new("powershell")
        .args(["-Command", &format!("Start-Process powershell -ArgumentList '-WindowStyle Hidden -Command \"{}\"' -Verb RunAs -Wait", script)])
        .output()
        .context("Failed to execute PowerShell for NRPT rule removal")?;

    if !output.status.success() {
        return Err(anyhow!("NRPT rule removal failed"));
    }

    Ok(())
}
