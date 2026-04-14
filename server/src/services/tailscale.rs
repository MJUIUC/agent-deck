use serde::Serialize;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Default)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub connected: bool,
    pub hostname: Option<String>,
    pub funnel_enabled: bool,
    pub funnel_url: Option<String>,
    pub auth_url: Option<String>,
    pub version: Option<String>,
    pub ip_address: Option<String>,
}

/// Query live Tailscale status. Never panics — errors are logged and converted
/// to a safe default TailscaleStatus with installed/connected = false.
pub async fn get_status(port: u16) -> TailscaleStatus {
    // Check if tailscale binary is available.
    let version_output = match Command::new("tailscale").arg("version").output().await {
        Ok(output) => output,
        Err(_) => {
            return TailscaleStatus {
                installed: false,
                ..Default::default()
            };
        }
    };

    if !version_output.status.success() {
        return TailscaleStatus {
            installed: false,
            ..Default::default()
        };
    }

    let version = String::from_utf8_lossy(&version_output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string());

    // Run `tailscale status --json` to get connection state.
    let status_output = match Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .await
    {
        Ok(output) => output,
        Err(_) => {
            return TailscaleStatus {
                installed: true,
                ..Default::default()
            };
        }
    };

    if !status_output.status.success() {
        return TailscaleStatus {
            installed: true,
            version,
            ..Default::default()
        };
    }

    let raw_json = String::from_utf8_lossy(&status_output.stdout);
    let parsed: serde_json::Value = match serde_json::from_str(&raw_json) {
        Ok(value) => value,
        Err(_) => {
            return TailscaleStatus {
                installed: true,
                version,
                ..Default::default()
            };
        }
    };

    let backend_state = parsed
        .get("BackendState")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let connected = backend_state == "Running";

    let hostname = parsed
        .get("Self")
        .and_then(|s| s.get("DNSName"))
        .and_then(|v| v.as_str())
        .map(|name| name.trim_end_matches('.').to_string());

    let ip_address = parsed
        .get("Self")
        .and_then(|s| s.get("TailscaleIPs"))
        .and_then(|ips| ips.as_array())
        .and_then(|ips| ips.first())
        .and_then(|v| v.as_str())
        .map(|ip| ip.to_string());

    let (funnel_enabled, funnel_url) = check_funnel(port).await;

    TailscaleStatus {
        installed: true,
        connected,
        hostname,
        funnel_enabled,
        funnel_url,
        auth_url: None,
        version,
        ip_address,
    }
}

/// Check whether Tailscale Funnel is enabled for the given port.
pub async fn check_funnel(port: u16) -> (bool, Option<String>) {
    let output = match Command::new("tailscale")
        .args(["funnel", "status"])
        .output()
        .await
    {
        Ok(output) => output,
        Err(_) => {
            return (false, None);
        }
    };

    if !output.status.success() {
        return (false, None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let port_str = port.to_string();

    // Check if the output mentions our port — if not, funnel is not active for it.
    if !stdout.contains(&port_str) {
        return (false, None);
    }

    // Extract the https:// URL from the output.
    for line in stdout.lines() {
        if line.contains("https://") {
            let url = line
                .split_whitespace()
                .find(|token| token.starts_with("https://"))
                .map(|token| token.trim_end_matches('/').to_string());
            if let Some(url) = url {
                return (true, Some(url));
            }
        }
    }

    // Port is referenced but no URL found — still treat as enabled.
    (true, None)
}

/// Run `tailscale up` and return updated status. Captures auth_url if present.
pub async fn run_connect(port: u16) -> TailscaleStatus {
    let output = match Command::new("tailscale")
        .args(["up", "--timeout=30s"])
        .output()
        .await
    {
        Ok(output) => output,
        Err(_) => {
            return TailscaleStatus {
                installed: true,
                ..Default::default()
            };
        }
    };

    // Combine stdout and stderr — the auth URL may appear in either.
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let auth_url = combined
        .lines()
        .flat_map(|line| line.split_whitespace())
        .find(|token| token.starts_with("https://login.tailscale.com/"))
        .map(|token| token.to_string());

    let mut status = get_status(port).await;
    status.auth_url = auth_url;
    status
}

/// Enable Tailscale Funnel for the given port.
pub async fn enable_funnel(port: u16) -> TailscaleStatus {
    let port_str = port.to_string();
    let output = Command::new("tailscale")
        .args(["funnel", &port_str])
        .output()
        .await;

    match output {
        Ok(result) if result.status.success() => {}
        Ok(_) | Err(_) => {}
    }

    get_status(port).await
}

/// Disable Tailscale Funnel for the given port.
pub async fn disable_funnel(port: u16) -> TailscaleStatus {
    // Try `tailscale funnel off` first (supported in newer versions).
    let output = Command::new("tailscale")
        .args(["funnel", "off"])
        .output()
        .await;

    match output {
        Ok(result) if result.status.success() => {}
        Ok(_) => {
            // Fall back to disabling for the specific port.
            let port_str = port.to_string();
            let _ = Command::new("tailscale")
                .args(["funnel", "--bg=false", &port_str])
                .output()
                .await;
        }
        Err(_) => {}
    }

    get_status(port).await
}
