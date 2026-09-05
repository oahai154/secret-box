//! SecretBox 核心库。
//!
//! 安全敏感逻辑都在这里：scrypt 密钥派生、AES-256-GCM 加解密、
//! SQLite 存储层的打开与读写。存储格式与 Go 版逐字节兼容（见 ADR-0002）。

pub mod crypto;
pub mod db;

pub use crypto::{decrypt, derive_key, encrypt, CryptoError};
pub use db::{Db, Item, SecretboxError, Version};
