//! 快照导出/导入的迁移文件格式。
//!
//! 与 Go 版 handlers.go 顶部注释定义的格式逐字节兼容：
//! `.secretbox` 文件内容 = base64( JSON{"version":1,"salt":"<b64 盐>","cipher":"<b64 密文>"} )，
//! cipher = Encrypt( DeriveKey(迁移口令, 随机盐), 序列化的 Snapshot JSON )。
//! 口令错误时解密失败，实现口令校验。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::db::SecretboxError;

/// 一次导出的完整数据快照（条目密文 + KEK 盐值 + DEK 包装），字段命名与
/// Go 版 Snapshot 一致；wrapped_dek 及恢复侧三字段为 v2 扩展（v1 旧快照
/// 反序列化为空，导入后仍走只读解锁路径）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(rename = "has_password")]
    pub has_password: bool,
    /// KEK 盐值（base64），导入后写回 meta 表。
    #[serde(rename = "salt")]
    pub salt_b64: String,
    /// DEK 的主密码侧包装（base64），v2 格式携带；v1 旧快照为空字符串。
    #[serde(rename = "wrapped_dek", default)]
    pub wrapped_dek_b64: String,
    /// 恢复密钥 KEK 盐值（base64），v2 且已设置恢复密钥时携带。
    #[serde(rename = "recovery_salt", default)]
    pub recovery_salt_b64: String,
    /// DEK 的恢复密钥侧包装（base64），v2 且已设置恢复密钥时携带。
    #[serde(rename = "wrapped_dek_recovery", default)]
    pub wrapped_dek_recovery_b64: String,
    /// DEK 加密的恢复密钥明文（base64），v2 且已设置恢复密钥时携带。
    #[serde(rename = "recovery_key_enc", default)]
    pub recovery_key_enc_b64: String,
    #[serde(default = "Vec::new")]
    pub items: Vec<SnapshotItem>,
}

/// 迁移文件中的单个条目（密文原样搬运，导入方无需迁移口令之外的密钥）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotItem {
    pub title: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// 已用主密码加密的密文。
    pub value: String,
    pub created: String,
    pub updated: String,
    #[serde(default = "Vec::new")]
    pub versions: Vec<SnapshotVersion>,
}

/// 迁移文件中的单个历史版本快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotVersion {
    pub version: i64,
    /// 已用主密码加密的密文快照。
    pub snapshot: String,
    pub created: String,
}

/// 建议的导出文件名，与 Go 版一致：secretbox-backup-<20060102-150405>.secretbox。
pub fn backup_filename() -> String {
    format!("secretbox-backup-{}.secretbox", Local::now().format("%Y%m%d-%H%M%S"))
}

/// 把快照用迁移口令加密成迁移文件内容（base64 文本，即写入 .secretbox 的内容）。
pub fn build_file(snap: &Snapshot, passphrase: &str) -> Result<String, SecretboxError> {
    let snap_json =
        serde_json::to_string(snap).map_err(|_| SecretboxError::SnapshotSerializeFailed)?;
    let (key, salt) = crypto::derive_key(passphrase, &[])?;
    let cipher = crypto::encrypt(&key, &snap_json)?;
    let file = serde_json::json!({
        "version": 1,
        "salt": BASE64.encode(&salt),
        "cipher": cipher,
    });
    Ok(BASE64.encode(file.to_string()))
}

/// 解析迁移文件内容：校验格式 → 派生密钥解密 → 反序列化快照。
/// 口令错误返回 [`SecretboxError::MigrationPassphraseWrong`]。
pub fn parse_file(content: &str, passphrase: &str) -> Result<Snapshot, SecretboxError> {
    let raw = BASE64
        .decode(content.trim())
        .map_err(|_| SecretboxError::MigrationFormatInvalid)?;
    let file: serde_json::Value = serde_json::from_slice(&raw)
        .map_err(|_| SecretboxError::MigrationFormatInvalid)?;
    if file.get("version").and_then(|v| v.as_i64()) != Some(1) {
        return Err(SecretboxError::MigrationFormatInvalid);
    }
    let salt_b64 = file
        .get("salt")
        .and_then(|v| v.as_str())
        .ok_or(SecretboxError::MigrationFormatInvalid)?;
    if salt_b64.is_empty() {
        return Err(SecretboxError::MigrationFormatInvalid);
    }
    let cipher = file
        .get("cipher")
        .and_then(|v| v.as_str())
        .ok_or(SecretboxError::MigrationFormatInvalid)?;
    let salt = BASE64
        .decode(salt_b64)
        .map_err(|_| SecretboxError::MigrationFormatInvalid)?;
    let (key, _) = crypto::derive_key(passphrase, &salt)?;
    let plain = crypto::decrypt(&key, cipher).map_err(|_| SecretboxError::MigrationPassphraseWrong)?;
    serde_json::from_str(&plain).map_err(|_| SecretboxError::MigrationContentInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 文件回环_口令错误被拒绝() {
        let snap = Snapshot {
            has_password: true,
            salt_b64: "abc".to_string(),
            wrapped_dek_b64: String::new(),
            recovery_salt_b64: String::new(),
            wrapped_dek_recovery_b64: String::new(),
            recovery_key_enc_b64: String::new(),
            items: vec![SnapshotItem {
                title: "t".into(),
                category: "".into(),
                note: "".into(),
                value: "cipher".into(),
                created: "2026-01-01T00:00:00+08:00".into(),
                updated: "2026-01-01T00:00:00+08:00".into(),
                versions: vec![],
            }],
        };
        let content = build_file(&snap, "口令1234").expect("构建成功");
        let parsed = parse_file(&content, "口令1234").expect("口令正确应解析成功");
        assert_eq!(parsed.items.len(), 1);
        assert_eq!(parsed.items[0].title, "t");
        assert!(parse_file(&content, "错误口令").is_err(), "口令错误必须失败");
    }
}
