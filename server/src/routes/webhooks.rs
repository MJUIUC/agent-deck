use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;

use crate::{error::AppResult, routes::AppState, services::encryption};

type HmacSha256 = Hmac<Sha256>;

#[derive(sqlx::FromRow)]
struct WebhookBinding {
    id: String,
    source: String,
    signature_header: Option<String>,
    prompt: String,
    secret: String,
}

/// `POST /api/webhooks`
///
/// Receives an inbound webhook delivery.  No session auth is required — the
/// HMAC signature in the request headers is the authentication mechanism.
///
/// The handler iterates over all enabled bindings, decrypts each stored secret,
/// and attempts to verify the request's HMAC signature.  The first binding
/// whose secret produces a valid signature wins.  If no binding matches the
/// request is rejected with 401.
pub async fn receive(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<impl IntoResponse> {
    let bindings: Vec<WebhookBinding> = sqlx::query_as(
        "SELECT id, source, signature_header, prompt, secret
         FROM webhook_bindings
         WHERE enabled = 1",
    )
    .fetch_all(&state.pool)
    .await?;

    if bindings.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let matched_binding = bindings.iter().find(|binding| {
        let plaintext_secret =
            match encryption::decrypt(&binding.secret, &state.credential_master_key) {
                Ok(s) => s,
                Err(_) => return false,
            };

        match binding.source.as_str() {
            "github" => verify_github_hmac(plaintext_secret.as_bytes(), &body, &headers),
            "gitlab" => verify_gitlab_signature(plaintext_secret.as_bytes(), &body, &headers),
            _ => {
                if let Some(ref header_name) = binding.signature_header {
                    verify_other_hmac(plaintext_secret.as_bytes(), &body, &headers, header_name)
                } else {
                    verify_generic_hmac(plaintext_secret.as_bytes(), &body, &headers)
                }
            }
        }
    });

    let binding = match matched_binding {
        Some(b) => b,
        None => return Ok(StatusCode::UNAUTHORIZED.into_response()),
    };

    let event_name = match binding.source.as_str() {
        "github" => headers
            .get("X-GitHub-Event")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string(),
        "gitlab" => headers
            .get("X-Gitlab-Event")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string(),
        _ => "webhook".to_string(),
    };

    let payload_json: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    let event_summary =
        crate::services::webhook_formatters::format(&payload_json, &binding.source, &event_name);

    // Load all thread attachments for this binding
    #[derive(sqlx::FromRow)]
    struct Attachment {
        thread_id: String,
        prompt: Option<String>,
    }
    let attachments: Vec<Attachment> = sqlx::query_as(
        "SELECT thread_id, prompt FROM thread_webhook_bindings WHERE webhook_binding_id = ?",
    )
    .bind(&binding.id)
    .fetch_all(&state.pool)
    .await?;

    // Fire notify_internal for each attached thread (fire-and-forget)
    for attachment in &attachments {
        let message = {
            // Per-thread prompt takes priority; fall back to binding's default prompt
            let instructions = if let Some(ref p) = attachment.prompt {
                let t = p.trim();
                if !t.is_empty() {
                    Some(t)
                } else {
                    None
                }
            } else {
                None
            }
            .or_else(|| {
                let t = binding.prompt.trim();
                if !t.is_empty() {
                    Some(t)
                } else {
                    None
                }
            });

            match instructions {
                Some(p) => format!("{}\n\n{}", event_summary, p),
                None => event_summary.clone(),
            }
        };
        // Ignore errors per-thread — one bad thread should not block others
        let _ = crate::routes::notify::notify_internal(
            &state,
            &attachment.thread_id,
            "webhook_executed",
            serde_json::Value::String(message),
        )
        .await;
    }

    Ok(StatusCode::ACCEPTED.into_response())
}

fn verify_github_hmac(secret: &[u8], body: &[u8], headers: &HeaderMap) -> bool {
    let signature_header = match headers
        .get("X-Hub-Signature-256")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => return false,
    };

    let hex_signature = match signature_header.strip_prefix("sha256=") {
        Some(s) => s,
        None => return false,
    };

    let expected_bytes = match hex::decode(hex_signature) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&expected_bytes).is_ok()
}

/// GitLab sends HMAC-SHA256 via `X-Gitlab-Signature-256: sha256=<hex>` on GitLab 15.2+.
/// Falls back to plaintext token comparison via `X-Gitlab-Token` for older instances.
fn verify_gitlab_signature(secret: &[u8], body: &[u8], headers: &HeaderMap) -> bool {
    // Prefer HMAC-SHA256 if the header is present
    if let Some(hmac_header) = headers
        .get("X-Gitlab-Signature-256")
        .and_then(|v| v.to_str().ok())
    {
        let hex_signature = match hmac_header.strip_prefix("sha256=") {
            Some(s) => s,
            None => return false,
        };
        let expected_bytes = match hex::decode(hex_signature) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let mut mac = match HmacSha256::new_from_slice(secret) {
            Ok(m) => m,
            Err(_) => return false,
        };
        mac.update(body);
        return mac.verify_slice(&expected_bytes).is_ok();
    }
    // Fall back to plaintext token comparison (older GitLab)
    if let Some(token) = headers.get("X-Gitlab-Token").and_then(|v| v.to_str().ok()) {
        // Constant-time comparison to avoid timing attacks
        let secret_str = match std::str::from_utf8(secret) {
            Ok(s) => s,
            Err(_) => return false,
        };
        return constant_time_eq(token.as_bytes(), secret_str.as_bytes());
    }
    false
}

/// For 'other' sources: reads the configured header name, decodes raw hex, verifies HMAC-SHA256.
/// No `sha256=` prefix is expected — the header value is a plain hex-encoded HMAC digest.
fn verify_other_hmac(secret: &[u8], body: &[u8], headers: &HeaderMap, header_name: &str) -> bool {
    let signature_value = match headers.get(header_name).and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        None => return false,
    };

    let expected_bytes = match hex::decode(signature_value) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&expected_bytes).is_ok()
}

fn verify_generic_hmac(secret: &[u8], body: &[u8], headers: &HeaderMap) -> bool {
    let signature_header = match headers
        .get("X-Webhook-Signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => return false,
    };

    let expected_bytes = match hex::decode(signature_header) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&expected_bytes).is_ok()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compute_github_hmac(secret: &[u8], body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
        mac.update(body);
        let result = mac.finalize();
        format!("sha256={}", hex::encode(result.into_bytes()))
    }

    fn compute_gitlab_hmac(secret: &[u8], body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
        mac.update(body);
        let result = mac.finalize();
        format!("sha256={}", hex::encode(result.into_bytes()))
    }

    fn compute_raw_hmac_hex(secret: &[u8], body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key size");
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    // ── GitHub tests ─────────────────────────────────────────────────────────

    #[test]
    fn verify_github_hmac_matches_known_vector() {
        let secret = b"my-webhook-secret";
        let body = b"{\"action\":\"opened\"}";
        let signature = compute_github_hmac(secret, body);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Hub-Signature-256",
            signature.parse().expect("valid header value"),
        );

        assert!(
            verify_github_hmac(secret, body, &headers),
            "HMAC should verify against a freshly-computed signature"
        );
    }

    #[test]
    fn verify_github_hmac_rejects_tampered_body() {
        let secret = b"my-webhook-secret";
        let original_body = b"{\"action\":\"opened\"}";
        let tampered_body = b"{\"action\":\"deleted\"}";

        let signature = compute_github_hmac(secret, original_body);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Hub-Signature-256",
            signature.parse().expect("valid header value"),
        );

        assert!(
            !verify_github_hmac(secret, tampered_body, &headers),
            "HMAC should reject a signature computed over a different body"
        );
    }

    #[test]
    fn verify_github_hmac_rejects_missing_header() {
        let secret = b"my-webhook-secret";
        let body = b"{}";
        let headers = HeaderMap::new();

        assert!(
            !verify_github_hmac(secret, body, &headers),
            "HMAC should reject when the signature header is absent"
        );
    }

    #[test]
    fn verify_github_hmac_rejects_wrong_prefix() {
        let secret = b"my-webhook-secret";
        let body = b"{}";

        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(body);
        let hex_sig = hex::encode(mac.finalize().into_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Hub-Signature-256",
            hex_sig.parse().expect("valid header value"),
        );

        assert!(
            !verify_github_hmac(secret, body, &headers),
            "HMAC should reject when 'sha256=' prefix is missing"
        );
    }

    // ── GitLab tests ──────────────────────────────────────────────────────────

    #[test]
    fn verify_gitlab_hmac_matches_known_vector() {
        let secret = b"gitlab-secret";
        let body = b"{\"object_kind\":\"push\"}";
        let signature = compute_gitlab_hmac(secret, body);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Gitlab-Signature-256",
            signature.parse().expect("valid header value"),
        );

        assert!(
            verify_gitlab_signature(secret, body, &headers),
            "GitLab HMAC should verify against a freshly-computed signature"
        );
    }

    #[test]
    fn verify_gitlab_plaintext_token_matches() {
        let secret = b"super-secret-token";
        let body = b"{\"object_kind\":\"issue\"}";

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Gitlab-Token",
            "super-secret-token".parse().expect("valid header value"),
        );

        assert!(
            verify_gitlab_signature(secret, body, &headers),
            "GitLab plaintext token should match when token equals secret"
        );
    }

    #[test]
    fn verify_gitlab_rejects_wrong_plaintext_token() {
        let secret = b"correct-token";
        let body = b"{\"object_kind\":\"issue\"}";

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Gitlab-Token",
            "wrong-token".parse().expect("valid header value"),
        );

        assert!(
            !verify_gitlab_signature(secret, body, &headers),
            "GitLab plaintext token should be rejected when it does not match the secret"
        );
    }

    #[test]
    fn verify_gitlab_hmac_takes_priority_over_plaintext() {
        let secret = b"shared-secret";
        let body = b"{\"object_kind\":\"merge_request\"}";

        // Compute a valid HMAC signature
        let valid_hmac = compute_gitlab_hmac(secret, body);

        let mut headers = HeaderMap::new();
        // Both headers present — HMAC should win and succeed
        headers.insert(
            "X-Gitlab-Signature-256",
            valid_hmac.parse().expect("valid header value"),
        );
        // Plaintext token is intentionally wrong to confirm HMAC took priority
        headers.insert(
            "X-Gitlab-Token",
            "totally-wrong-token".parse().expect("valid header value"),
        );

        assert!(
            verify_gitlab_signature(secret, body, &headers),
            "HMAC header should take priority over plaintext token header"
        );
    }

    #[test]
    fn verify_gitlab_rejects_when_no_headers_present() {
        let secret = b"some-secret";
        let body = b"{}";
        let headers = HeaderMap::new();

        assert!(
            !verify_gitlab_signature(secret, body, &headers),
            "GitLab verification should fail when neither signature header is present"
        );
    }

    #[test]
    fn verify_gitlab_hmac_rejects_tampered_body() {
        let secret = b"gitlab-secret";
        let original_body = b"{\"object_kind\":\"push\"}";
        let tampered_body = b"{\"object_kind\":\"tag_push\"}";

        let signature = compute_gitlab_hmac(secret, original_body);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Gitlab-Signature-256",
            signature.parse().expect("valid header value"),
        );

        assert!(
            !verify_gitlab_signature(secret, tampered_body, &headers),
            "GitLab HMAC should reject a signature computed over a different body"
        );
    }

    // ── verify_other_hmac tests ───────────────────────────────────────────────

    #[test]
    fn verify_other_hmac_matches_known_vector() {
        let secret = b"linear-secret";
        let body = b"{\"type\":\"Issue\",\"action\":\"create\"}";
        let hex_sig = compute_raw_hmac_hex(secret, body);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Linear-Signature",
            hex_sig.parse().expect("valid header value"),
        );

        assert!(
            verify_other_hmac(secret, body, &headers, "X-Linear-Signature"),
            "other HMAC should verify against a freshly-computed raw hex signature"
        );
    }

    #[test]
    fn verify_other_hmac_rejects_tampered_body() {
        let secret = b"linear-secret";
        let original_body = b"{\"type\":\"Issue\",\"action\":\"create\"}";
        let tampered_body = b"{\"type\":\"Issue\",\"action\":\"delete\"}";
        let hex_sig = compute_raw_hmac_hex(secret, original_body);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Linear-Signature",
            hex_sig.parse().expect("valid header value"),
        );

        assert!(
            !verify_other_hmac(secret, tampered_body, &headers, "X-Linear-Signature"),
            "other HMAC should reject a signature computed over a different body"
        );
    }

    #[test]
    fn verify_other_hmac_rejects_missing_header() {
        let secret = b"linear-secret";
        let body = b"{}";
        let headers = HeaderMap::new();

        assert!(
            !verify_other_hmac(secret, body, &headers, "X-Linear-Signature"),
            "other HMAC should reject when the configured header is absent"
        );
    }

    #[test]
    fn verify_other_hmac_rejects_wrong_header_name() {
        let secret = b"linear-secret";
        let body = b"{\"type\":\"Issue\"}";
        let hex_sig = compute_raw_hmac_hex(secret, body);

        let mut headers = HeaderMap::new();
        // Signature is in X-Linear-Signature but we look for X-Other-Signature
        headers.insert(
            "X-Linear-Signature",
            hex_sig.parse().expect("valid header value"),
        );

        assert!(
            !verify_other_hmac(secret, body, &headers, "X-Other-Signature"),
            "other HMAC should reject when looking at a different header name"
        );
    }

    // ── constant_time_eq tests ────────────────────────────────────────────────

    #[test]
    fn constant_time_eq_returns_true_for_equal_slices() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn constant_time_eq_returns_false_for_different_slices() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn constant_time_eq_returns_false_for_different_lengths() {
        assert!(!constant_time_eq(b"short", b"much-longer-string"));
    }
}
