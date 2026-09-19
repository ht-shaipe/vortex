//! AES-256-GCM 加密/解密、密钥派生与密钥生成。
//!
//! 提供敏感字段（如 API Key、Access Token）的对称加密保护。
//! 加密时随机生成 12 字节 Nonce，与密文拼接后 Base64 编码输出；
//! 解密时反向拆分 Nonce 与密文进行解密。密钥派生使用自定义混合算法将口令扩展为 32 字节密钥。

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};

/// GCM Nonce 长度（12 字节）
const NONCE_SIZE: usize = 12;

/// 使用 AES-256-GCM 加密明文。
///
/// 随机生成 12 字节 Nonce，加密后将 Nonce 与密文拼接，再做 Base64 编码输出。
/// 每次加密结果不同（因 Nonce 随机），但均可由相同密钥解密。
///
/// # 参数
/// - `key`：32 字节加密密钥
/// - `plaintext`：待加密的明文字符串
///
/// # 返回
/// 成功返回 Base64 编码的密文字符串，失败返回错误描述
pub fn encrypt(key: &[u8], plaintext: &str) -> Result<String, String> {
    // 初始化 AES-256-GCM 加密器
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Key init error: {}", e))?;
    // 生成随机 Nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    // 加密明文
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption error: {}", e))?;
    // 拼接 Nonce + 密文
    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    // Base64 编码输出
    Ok(B64.encode(combined))
}

/// 使用 AES-256-GCM 解密密文。
///
/// 先 Base64 解码，拆分出前 12 字节 Nonce 和剩余密文，再执行解密。
///
/// # 参数
/// - `key`：32 字节加密密钥（须与加密时一致）
/// - `ciphertext_b64`：Base64 编码的密文（含 Nonce 前缀）
///
/// # 返回
/// 成功返回解密后的明文字符串，失败返回错误描述
pub fn decrypt(key: &[u8], ciphertext_b64: &str) -> Result<String, String> {
    // 初始化 AES-256-GCM 解密器
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("Key init error: {}", e))?;
    // Base64 解码
    let combined = B64
        .decode(ciphertext_b64)
        .map_err(|e| format!("Base64 decode error: {}", e))?;
    if combined.len() < NONCE_SIZE {
        return Err("Ciphertext too short".into());
    }
    // 拆分 Nonce 与密文
    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);
    // 解密
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {}", e))?;
    // 转为 UTF-8 字符串
    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 error: {}", e))
}

/// 从口令派生 32 字节加密密钥。
///
/// 采用自定义混合算法：先将口令字节循环填充到 32 字节数组，
/// 再经过 1000 轮叠加轮次常量与循环左移的混淆操作，输出 32 字节密钥。
///
/// 注意：此为轻量派生，非标准 KDF（如 PBKDF2/Argon2），适用于本地存储场景。
///
/// # 参数
/// - `passphrase`：用户口令字符串
///
/// # 返回
/// 32 字节密钥向量
pub fn derive_key(passphrase: &str) -> Vec<u8> {
    let mut key = [0u8; 32];
    let passphrase_bytes = passphrase.as_bytes();
    // 第一阶段：将口令字节循环填充到 32 字节
    let mut i = 0usize;
    while i < 32 {
        for &b in passphrase_bytes {
            if i >= 32 { break; }
            key[i] = key[i].wrapping_add(b);
            i += 1;
        }
    }
    // 第二阶段：1000 轮混淆（叠加轮次常量 + 循环左移）
    for round in 0..1000u32 {
        let round_bytes = round.to_le_bytes();
        for i in 0..32 {
            key[i] = key[i].wrapping_add(round_bytes[i % 4]);
            key[i] = key[i].rotate_left(3);
        }
    }
    key.to_vec()
}

/// 生成 32 字节随机加密密钥并返回十六进制字符串。
///
/// 使用 `OsRng` 系统级随机源生成 32 字节（256 位）密钥，输出为 64 字符十六进制字符串。
///
/// # 返回
/// 64 字符十六进制密钥字符串
pub fn generate_encryption_key() -> String {
    use rand::RngCore;
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    hex::encode(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证加密后解密能还原原文
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = derive_key("test-passphrase");
        let plaintext = "sk-1234567890abcdef";
        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(plaintext, decrypted);
    }

    /// 验证相同明文两次加密结果不同（因 Nonce 随机）
    #[test]
    fn test_different_nonces() {
        let key = derive_key("test-passphrase");
        let plaintext = "same-text";
        let e1 = encrypt(&key, plaintext).unwrap();
        let e2 = encrypt(&key, plaintext).unwrap();
        assert_ne!(e1, e2);
    }
}
