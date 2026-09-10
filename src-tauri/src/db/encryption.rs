use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};

const NONCE_SIZE: usize = 12;

pub fn encrypt(key: &[u8], plaintext: &str) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Key init error: {}", e))?;
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption error: {}", e))?;
    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(combined))
}

pub fn decrypt(key: &[u8], ciphertext_b64: &str) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Key init error: {}", e))?;
    let combined = B64
        .decode(ciphertext_b64)
        .map_err(|e| format!("Base64 decode error: {}", e))?;
    if combined.len() < NONCE_SIZE {
        return Err("Ciphertext too short".into());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {}", e))?;
    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 error: {}", e))
}

pub fn derive_key(passphrase: &str) -> Vec<u8> {
    let mut key = [0u8; 32];
    let passphrase_bytes = passphrase.as_bytes();
    let mut i = 0usize;
    while i < 32 {
        for &b in passphrase_bytes {
            if i >= 32 { break; }
            key[i] = key[i].wrapping_add(b);
            i += 1;
        }
    }
    for round in 0..1000u32 {
        let round_bytes = round.to_le_bytes();
        for i in 0..32 {
            key[i] = key[i].wrapping_add(round_bytes[i % 4]);
            key[i] = key[i].rotate_left(3);
        }
    }
    key.to_vec()
}

pub fn generate_encryption_key() -> String {
    use rand::RngCore;
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    hex::encode(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = derive_key("test-passphrase");
        let plaintext = "sk-1234567890abcdef";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_different_nonces() {
        let key = derive_key("test-passphrase");
        let plaintext = "same-text";
        let e1 = encrypt(&key, plaintext).unwrap();
        let e2 = encrypt(&key, plaintext).unwrap();
        assert_ne!(e1, e2);
    }
}
