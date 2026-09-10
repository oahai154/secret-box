//! SecretBox 核心库。
//!
//! 安全敏感逻辑都在这里：scrypt 密钥派生、AES-256-GCM 加解密、
//! SQLite 存储层的打开与读写。v2 起条目由随机 DEK 加密、主密码只负责
//! 包装 DEK（见 ADR-0003）；v1 旧库为只读导入源。

pub mod crypto;
pub mod db;
pub mod export;
pub mod migration;
pub mod recovery;

pub use crypto::{decrypt, derive_key, encrypt, CryptoError};
pub use db::{Db, Item, SecretboxError, Version};
pub use export::{build_csv, plaintext_filename};
pub use migration::{backup_filename, build_file, parse_file, Snapshot, SnapshotItem, SnapshotVersion};
pub use recovery::{generate_recovery_key, normalize_recovery_key};
