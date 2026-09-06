//! 加密层：scrypt 密钥派生 + AES-256-GCM 加解密。
//!
//! 条目密文布局与 Go 版一致：base64( nonce(12 字节) || ciphertext+tag )，标准 base64。
//! scrypt 参数 N=1<<15, r=8, p=1, 输出 32 字节。
//!
//! v2 起条目由随机 DEK 加密（见 ADR-0003）：主密码经 scrypt 派生 KEK 后
//! 仅用于包装/解开 DEK（[`encrypt_bytes`]/[`decrypt_bytes`]），不再直接加密条目。

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::rngs::OsRng;
use rand::TryRngCore;
use scrypt::Params;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("密钥派生失败")]
    DeriveFailed,
    #[error("密文长度不足")]
    CiphertextTooShort,
    #[error("解密失败(主密码可能不正确)")]
    DecryptFailed,
    #[error("加密失败")]
    EncryptFailed,
    #[error("base64 解码失败")]
    Base64DecodeFailed,
}

/// 盐值长度：32 字节。
pub const SALT_SIZE: usize = 32;
/// GCM nonce 长度：12 字节。
pub const NONCE_SIZE: usize = 12;
/// AES-256 密钥长度：32 字节。
pub const KEY_LEN: usize = 32;

/// scrypt 参数，与 Go 版一致：N=1<<15, r=8, p=1。
fn scrypt_params() -> Params {
    // N=2^15 对应 log_n=15
    Params::new(15, 8, 1, KEY_LEN).expect("固定的 scrypt 参数必然合法")
}

/// 从主密码派生 AES-256 密钥。
///
/// 传入 salt 为空时生成新随机盐。返回 (密钥, 盐值)。
/// 密钥只应存于内存，绝不落盘。
pub fn derive_key(password: &str, salt: &[u8]) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
    let salt = if salt.is_empty() {
        let mut generated = vec![0u8; SALT_SIZE];
        OsRng
            .try_fill_bytes(&mut generated)
            .map_err(|_| CryptoError::DeriveFailed)?;
        generated
    } else {
        salt.to_vec()
    };

    let mut key = vec![0u8; KEY_LEN];
    scrypt::scrypt(password.as_bytes(), &salt, &scrypt_params(), &mut key)
        .map_err(|_| CryptoError::DeriveFailed)?;
    Ok((key, salt))
}

/// 生成 n 字节加密随机数（DEK、盐值等密钥材料）。
pub fn random_bytes(n: usize) -> Result<Vec<u8>, CryptoError> {
    let mut buf = vec![0u8; n];
    OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|_| CryptoError::DeriveFailed)?;
    Ok(buf)
}

/// AES-256-GCM 加密，返回 base64 编码字符串：base64( nonce || ciphertext+tag )。
pub fn encrypt(key: &[u8], plaintext: &str) -> Result<String, CryptoError> {
    encrypt_bytes(key, plaintext.as_bytes())
}

/// AES-256-GCM 加密原始字节（用于包装 DEK 等二进制密钥材料）。
pub fn encrypt_bytes(key: &[u8], plaintext: &[u8]) -> Result<String, CryptoError> {
    let cipher = new_cipher(key)?;
    let nonce_bytes = random_bytes(NONCE_SIZE)?;

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| CryptoError::EncryptFailed)?;

    let mut out = nonce_bytes;
    out.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(out))
}

/// 解密 base64 编码的 GCM 密文。密钥错误将返回 [`CryptoError::DecryptFailed`]。
pub fn decrypt(key: &[u8], encoded: &str) -> Result<String, CryptoError> {
    let plain = decrypt_bytes(key, encoded)?;
    String::from_utf8(plain).map_err(|_| CryptoError::DecryptFailed)
}

/// 解密 base64 编码的 GCM 密文为原始字节（用于解开 DEK 包装）。
pub fn decrypt_bytes(key: &[u8], encoded: &str) -> Result<Vec<u8>, CryptoError> {
    let raw = BASE64
        .decode(encoded)
        .map_err(|_| CryptoError::Base64DecodeFailed)?;
    if raw.len() < NONCE_SIZE {
        return Err(CryptoError::CiphertextTooShort);
    }
    let cipher = new_cipher(key)?;
    let (nonce, ciphertext) = raw.split_at(NONCE_SIZE);
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| CryptoError::DecryptFailed)
}

fn new_cipher(key: &[u8]) -> Result<Aes256Gcm, CryptoError> {
    let key_bytes: &[u8; KEY_LEN] = key
        .try_into()
        .map_err(|_| CryptoError::DeriveFailed)?;
    Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_bytes)))
}
