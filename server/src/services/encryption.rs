use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};

/// Derive a 32-byte AES-256 key from a machine-specific secret string.
/// The secret is typically a combination of machine identifiers or a stored
/// random seed in app_config.
fn derive_key(secret: &str) -> Key<Aes256Gcm> {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-deck-encryption-v1:");
    hasher.update(secret.as_bytes());
    let result = hasher.finalize();
    *Key::<Aes256Gcm>::from_slice(&result)
}

/// Encrypt plaintext using AES-256-GCM.
/// Returns a base64-encoded string of `nonce (12 bytes) || ciphertext`.
pub fn encrypt(plaintext: &str, secret: &str) -> Result<String> {
    let key = derive_key(secret);
    let cipher = Aes256Gcm::new(&key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow!("Encryption failed: {}", e))?;

    // Prepend nonce to ciphertext, then base64-encode the whole thing
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

/// Decrypt a base64-encoded `nonce || ciphertext` string produced by `encrypt`.
pub fn decrypt(encoded: &str, secret: &str) -> Result<String> {
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| anyhow!("Base64 decode failed: {}", e))?;

    if combined.len() < 12 {
        return Err(anyhow!("Ciphertext too short"));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = derive_key(secret);
    let cipher = Aes256Gcm::new(&key);

    let plaintext_bytes = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext_bytes).map_err(|e| anyhow!("UTF-8 decode failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret = "test-machine-secret-12345";
        let plaintext = "sk-supersecretapikey";

        let encrypted = encrypt(plaintext, secret).expect("encrypt should succeed");
        assert_ne!(
            encrypted, plaintext,
            "encrypted should differ from plaintext"
        );

        let decrypted = decrypt(&encrypted, secret).expect("decrypt should succeed");
        assert_eq!(decrypted, plaintext, "decrypted should match original");
    }

    #[test]
    fn test_different_encryptions_produce_different_ciphertext() {
        let secret = "test-machine-secret";
        let plaintext = "same-api-key";

        let enc1 = encrypt(plaintext, secret).expect("first encrypt");
        let enc2 = encrypt(plaintext, secret).expect("second encrypt");

        // Each encryption uses a random nonce, so results must differ
        assert_ne!(
            enc1, enc2,
            "two encryptions of same plaintext should differ"
        );
    }

    #[test]
    fn test_wrong_secret_fails_decryption() {
        let plaintext = "secret-key";
        let encrypted = encrypt(plaintext, "correct-secret").expect("encrypt");
        let result = decrypt(&encrypted, "wrong-secret");
        assert!(result.is_err(), "decryption with wrong secret should fail");
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let plaintext = "secret-key";
        let mut encrypted = encrypt(plaintext, "secret").expect("encrypt");
        // Tamper with the last character
        let last = encrypted.pop().unwrap();
        let replacement = if last == 'A' { 'B' } else { 'A' };
        encrypted.push(replacement);

        let result = decrypt(&encrypted, "secret");
        assert!(
            result.is_err(),
            "tampered ciphertext should fail to decrypt"
        );
    }
}
