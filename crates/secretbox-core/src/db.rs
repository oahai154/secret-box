//! 数据层：SQLite 打开/初始化 + 条目与历史版本的读取。
//!
//! 与 Go 版 db.go 对齐：
//! - 打开时启用 WAL 与外键；
//! - 表结构：meta / secret_items / secret_versions / settings；
//! - 盐值存于 meta 表 key='salt'，base64 编码。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::crypto::{self, CryptoError};

#[derive(Debug, thiserror::Error)]
pub enum SecretboxError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("base64 解码失败")]
    Base64DecodeFailed,
    #[error("加密错误: {0}")]
    Crypto(#[from] CryptoError),
    #[error("条目不存在")]
    ItemNotFound,
    #[error("版本不存在")]
    VersionNotFound,
}

/// 条目（读列表时不含明文；用 [`Db::get_item`] 单独读取时填充 `value`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    /// 列表附带的历史版本数量。
    #[serde(default, skip_serializing_if = "is_zero")]
    pub version_count: i64,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

/// 单个历史版本（仅元数据）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub id: i64,
    pub secret_id: i64,
    pub version: i64,
    pub created_at: String,
}

/// 持有数据库连接与运行时元信息盐值。
pub struct Db {
    conn: Connection,
    db_path: String,
    /// 主密码派生密钥所用盐值，从 meta 表读取。
    salt: Option<Vec<u8>>,
}

/// SQLite 打开参数与 Go 版 DSN 一致：WAL 日志 + 外键约束。
fn apply_pragmas(conn: &Connection) -> Result<(), SecretboxError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// 建表（与 Go 版 initTables 语句一致）。
fn init_tables(conn: &Connection) -> Result<(), SecretboxError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS secret_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT '',
            note TEXT NOT NULL DEFAULT '',
            encrypted_value TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS secret_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            secret_id INTEGER NOT NULL REFERENCES secret_items(id) ON DELETE CASCADE,
            version INTEGER NOT NULL,
            encrypted_snapshot TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_versions_secret ON secret_versions(secret_id, version DESC);
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// 为旧版数据库添加 note 列（若不存在），与 Go 版迁移一致。
fn migrate_add_note_column(conn: &Connection) -> Result<(), SecretboxError> {
    let has_note: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('secret_items') WHERE name='note'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n > 0)?;
    if !has_note {
        conn.execute_batch(
            "ALTER TABLE secret_items ADD COLUMN note TEXT NOT NULL DEFAULT '';",
        )?;
    }
    Ok(())
}

impl Db {
    /// 打开或创建数据库文件，必要时初始化表并读取盐值。
    pub fn open(db_path: &str) -> Result<Db, SecretboxError> {
        let conn = Connection::open(db_path)?;
        apply_pragmas(&conn)?;
        init_tables(&conn)?;
        migrate_add_note_column(&conn)?;

        let mut db = Db {
            conn,
            db_path: db_path.to_string(),
            salt: None,
        };
        db.reload_salt()?;
        Ok(db)
    }

    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    /// 从 meta 表重新读取盐值。
    pub fn reload_salt(&mut self) -> Result<(), SecretboxError> {
        self.salt = match self
            .conn
            .query_row("SELECT value FROM meta WHERE key='salt'", [], |row| {
                row.get::<_, String>(0)
            }) {
            Ok(encoded) => Some(
                BASE64
                    .decode(&encoded)
                    .map_err(|_| SecretboxError::Base64DecodeFailed)?,
            ),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(err) => return Err(err.into()),
        };
        Ok(())
    }

    /// 是否已设置主密码（依据盐值是否已写）。
    pub fn has_master_password(&self) -> bool {
        self.salt.is_some()
    }

    /// 用主密码解锁：派生密钥并用首条密文验证（无数据时跳过校验，仍视为成功）。
    /// 成功返回派生密钥（只应存于内存）。
    pub fn unlock(&self, password: &str) -> Result<Vec<u8>, SecretboxError> {
        let salt = self
            .salt
            .as_ref()
            .ok_or(SecretboxError::Crypto(CryptoError::DeriveFailed))?;
        let (key, _) = crypto::derive_key(password, salt)?;
        if let Some(encrypted) = self.get_any_encrypted() {
            crypto::decrypt(&key, &encrypted)?;
        }
        Ok(key)
    }

    /// 返回任一条目的密文用于解锁校验；无数据返回 None。
    /// 优先取 secret_items，若空则取任意历史快照。
    pub fn get_any_encrypted(&self) -> Option<String> {
        match self
            .conn
            .query_row("SELECT encrypted_value FROM secret_items LIMIT 1", [], |row| {
                row.get::<_, String>(0)
            }) {
            Ok(encrypted) => Some(encrypted),
            Err(_) => self
                .conn
                .query_row(
                    "SELECT encrypted_snapshot FROM secret_versions LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok(),
        }
    }

    /// 返回全部条目（不含明文）及版本数量，按更新时间倒序。
    pub fn list_items(&self) -> Result<Vec<Item>, SecretboxError> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.category, s.note, s.created_at, s.updated_at,
                    (SELECT COUNT(*) FROM secret_versions v WHERE v.secret_id = s.id) AS vcount
             FROM secret_items s ORDER BY s.updated_at DESC",
        )?;
        let items = stmt
            .query_map([], |row| {
                Ok(Item {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    category: row.get(2)?,
                    note: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    value: String::new(),
                    version_count: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(items)
    }

    /// 读取单条并返回解密明文。
    pub fn get_item(&self, key: &[u8], id: i64) -> Result<Item, SecretboxError> {
        let (mut item, encrypted) = self.query_item(id)?;
        item.value = crypto::decrypt(key, &encrypted)?;
        Ok(item)
    }

    /// 返回某条目全部版本（仅元数据），按版本号倒序。
    pub fn list_versions(&self, secret_id: i64) -> Result<Vec<Version>, SecretboxError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, secret_id, version, created_at
             FROM secret_versions WHERE secret_id = ?1 ORDER BY version DESC",
        )?;
        let versions = stmt
            .query_map([secret_id], |row| {
                Ok(Version {
                    id: row.get(0)?,
                    secret_id: row.get(1)?,
                    version: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(versions)
    }

    /// 读取某历史版本的加密快照并解密。
    pub fn get_version_snapshot(
        &self,
        key: &[u8],
        secret_id: i64,
        version: i64,
    ) -> Result<String, SecretboxError> {
        let encrypted: String = self
            .conn
            .query_row(
                "SELECT encrypted_snapshot FROM secret_versions WHERE secret_id = ?1 AND version = ?2",
                [secret_id, version],
                |row| row.get(0),
            )
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => SecretboxError::VersionNotFound,
                other => other.into(),
            })?;
        Ok(crypto::decrypt(key, &encrypted)?)
    }

    /// 读取单条的元数据与密文（value 不解密）。
    fn query_item(&self, id: i64) -> Result<(Item, String), SecretboxError> {
        self.conn
            .query_row(
                "SELECT id, title, category, note, encrypted_value, created_at, updated_at
                 FROM secret_items WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        Item {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            category: row.get(2)?,
                            note: row.get(3)?,
                            created_at: row.get(5)?,
                            updated_at: row.get(6)?,
                            value: String::new(),
                            version_count: 0,
                        },
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .map_err(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => SecretboxError::ItemNotFound,
                other => other.into(),
            })
    }
}
