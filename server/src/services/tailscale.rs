use serde::Serialize;
use tokio::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Default)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub connected: bool,
    pub needs_service: bool,
    pub serving: bool,
    pub serve_url: Option<String>,
    pub hostname: Option<String>,
    pub funnel_enabled: bool,
    pub funnel_url: Option<String>,
    pub auth_url: Option<String>,
    pub version: Option<String>,
    pub ip_address: Option<String>,
    pub message: Option<String>,
}

fn is_service_not_running(stderr: &str) -> bool {
    stderr.contains("failed to connect to local Tailscale service")
}

/// Parse the JSON output of `tailscale status --json` into (connected, hostname, ip_address).
pub(crate) fn parse_status_json(json: &str) -> (bool, Option<String>, Option<String>) {
    let parsed: serde_json::Value = match serde_json::from_str(json) {
        Ok(value) => value,
        Err(_) => return (false, None, None),
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

    (connected, hostname, ip_address)
}

/// Extract the first `https://` URL from `tailscale funnel status` text output.
pub(crate) fn extract_funnel_url(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.contains("https://") {
            let url = line
                .split_whitespace()
                .find(|token| token.starts_with("https://"))
                .map(|token| token.trim_end_matches('/').to_string());
            if url.is_some() {
                return url;
            }
        }
    }
    None
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
        let stderr = String::from_utf8_lossy(&status_output.stderr).to_string();
        let needs_service = is_service_not_running(&stderr);
        return TailscaleStatus {
            installed: true,
            needs_service,
            version,
            ..Default::default()
        };
    }

    let raw_json = String::from_utf8_lossy(&status_output.stdout);
    let (connected, hostname, ip_address) = {
        let result = parse_status_json(&raw_json);
        if raw_json.trim().is_empty()
            || serde_json::from_str::<serde_json::Value>(&raw_json).is_err()
        {
            return TailscaleStatus {
                installed: true,
                version,
                ..Default::default()
            };
        }
        result
    };

    let (funnel_enabled, funnel_url) = check_funnel(port).await;

    let (serving, serve_url) = if connected {
        check_serve(port).await
    } else {
        (false, None)
    };

    TailscaleStatus {
        installed: true,
        connected,
        needs_service: false,
        serving,
        serve_url,
        hostname,
        funnel_enabled,
        funnel_url,
        auth_url: None,
        version,
        ip_address,
        message: None,
    }
}

/// Check whether `tailscale serve` is active for the given port.
pub async fn check_serve(port: u16) -> (bool, Option<String>) {
    let output = match Command::new("tailscale")
        .args(["serve", "status"])
        .output()
        .await
    {
        Ok(output) => output,
        Err(_) => return (false, None),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}{}", stdout, stderr);
    let port_str = port.to_string();

    if !combined.contains(&port_str) {
        return (false, None);
    }

    for line in combined.lines() {
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

    (true, None)
}

/// Enable `tailscale serve` in the background for the given port.
pub async fn enable_serve(port: u16) -> TailscaleStatus {
    info!("tailscale: enabling serve on port {}", port);
    let port_str = port.to_string();
    let output = Command::new("tailscale")
        .args(["serve", "--bg", &port_str])
        .output()
        .await;

    match output {
        Ok(result) if result.status.success() => {
            info!("tailscale: serve enabled on port {}", port);
        }
        Ok(result) => {
            warn!(
                "tailscale: 'tailscale serve --bg {}' failed ({:?}): {}",
                port,
                result.status.code(),
                String::from_utf8_lossy(&result.stderr).trim()
            );
        }
        Err(e) => {
            warn!("tailscale: failed to spawn 'tailscale serve': {}", e);
        }
    }

    get_status(port).await
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
    let url = extract_funnel_url(&stdout);
    if url.is_some() {
        return (true, url);
    }

    // Port is referenced but no URL found — still treat as enabled.
    (true, None)
}

/// Run `tailscale up` and return updated status. Captures auth_url if present.
pub async fn run_connect(port: u16) -> TailscaleStatus {
    info!("tailscale: running 'tailscale up --timeout=5s'");

    let output = match Command::new("tailscale")
        .args(["up", "--timeout=5s"])
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            warn!("tailscale: failed to spawn 'tailscale up': {}", e);
            return TailscaleStatus {
                installed: true,
                message: Some(format!("Failed to run tailscale: {}", e)),
                ..Default::default()
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    if output.status.success() {
        info!("tailscale: 'tailscale up' succeeded");
    } else {
        info!(
            "tailscale: 'tailscale up' exited {:?} — stderr: {}",
            output.status.code(),
            stderr.trim()
        );
    }

    let auth_url = combined
        .lines()
        .flat_map(|line| line.split_whitespace())
        .find(|token| token.starts_with("https://login.tailscale.com/"))
        .map(|token| token.to_string());

    if let Some(ref url) = auth_url {
        info!("tailscale: auth URL returned: {}", url);
    }

    let mut status = get_status(port).await;
    status.auth_url = auth_url.clone();

    if status.connected && !status.serving {
        info!(
            "tailscale: connected — auto-starting serve on port {}",
            port
        );
        let mut served = enable_serve(port).await;
        served.auth_url = auth_url;
        return served;
    }

    // If still not connected and no auth URL, surface the stderr as a message
    // so the frontend can show the user what went wrong.
    if !status.connected && status.auth_url.is_none() {
        let trimmed = stderr.trim().to_string();
        if is_service_not_running(&trimmed) {
            status.needs_service = true;
        } else if !trimmed.is_empty() {
            status.message = Some(trimmed);
        }
    }

    status
}

/// Enable Tailscale Funnel for the given port.
pub async fn enable_funnel(port: u16) -> TailscaleStatus {
    info!("tailscale: enabling funnel on port {}", port);
    let port_str = port.to_string();
    let output = Command::new("tailscale")
        .args(["funnel", &port_str])
        .output()
        .await;

    match output {
        Ok(result) if result.status.success() => {
            info!("tailscale: funnel enabled on port {}", port);
        }
        Ok(result) => {
            warn!(
                "tailscale: 'tailscale funnel {}' failed ({:?}): {}",
                port,
                result.status.code(),
                String::from_utf8_lossy(&result.stderr).trim()
            );
        }
        Err(e) => {
            warn!("tailscale: failed to spawn 'tailscale funnel': {}", e);
        }
    }

    get_status(port).await
}

#[cfg(test)]
mod tests {
    use super::{extract_funnel_url, parse_status_json};

    #[test]
    fn parse_status_connected() {
        let json = r#"{
            "BackendState": "Running",
            "Self": {
                "DNSName": "mac-mini.tail1234.ts.net.",
                "TailscaleIPs": ["100.64.1.2"]
            }
        }"#;
        let (connected, hostname, ip_address) = parse_status_json(json);
        assert!(connected);
        assert_eq!(hostname, Some("mac-mini.tail1234.ts.net".to_string()));
        assert_eq!(ip_address, Some("100.64.1.2".to_string()));
    }

    #[test]
    fn parse_status_disconnected() {
        let json = r#"{
            "BackendState": "Stopped",
            "Self": {
                "DNSName": "mac-mini.tail1234.ts.net.",
                "TailscaleIPs": ["100.64.1.2"]
            }
        }"#;
        let (connected, _, _) = parse_status_json(json);
        assert!(!connected);
    }

    #[test]
    fn parse_status_not_running() {
        let json = r#"{
            "BackendState": "NeedsLogin",
            "Self": {
                "DNSName": "mac-mini.tail1234.ts.net.",
                "TailscaleIPs": ["100.64.1.2"]
            }
        }"#;
        let (connected, _, _) = parse_status_json(json);
        assert!(!connected);
    }

    #[test]
    fn extract_funnel_url_present() {
        let output = "\
# Funnel on:\n\
  - https://mac-mini.tail1234.ts.net 443 -> 8080\n\
";
        let url = extract_funnel_url(output);
        assert_eq!(url, Some("https://mac-mini.tail1234.ts.net".to_string()));
    }

    #[test]
    fn extract_funnel_url_absent() {
        let output = "\
# Funnel off\n\
No tunnels active.\n\
";
        let url = extract_funnel_url(output);
        assert_eq!(url, None);
    }
}

/// Disable Tailscale Funnel for the given port.
pub async fn disable_funnel(port: u16) -> TailscaleStatus {
    info!("tailscale: disabling funnel");
    let output = Command::new("tailscale")
        .args(["funnel", "off"])
        .output()
        .await;

    match output {
        Ok(result) if result.status.success() => {
            info!("tailscale: funnel disabled via 'funnel off'");
        }
        Ok(_) => {
            // Fall back to disabling for the specific port.
            info!(
                "tailscale: 'funnel off' unavailable, falling back to 'funnel --bg=false {}'",
                port
            );
            let port_str = port.to_string();
            match Command::new("tailscale")
                .args(["funnel", "--bg=false", &port_str])
                .output()
                .await
            {
                Ok(result) if result.status.success() => {
                    info!("tailscale: funnel disabled via fallback");
                }
                Ok(result) => {
                    warn!(
                        "tailscale: fallback disable failed ({:?}): {}",
                        result.status.code(),
                        String::from_utf8_lossy(&result.stderr).trim()
                    );
                }
                Err(e) => {
                    warn!("tailscale: failed to spawn fallback funnel disable: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("tailscale: failed to spawn 'tailscale funnel off': {}", e);
        }
    }

    get_status(port).await
}
