//! 数据层：SQLite 打开/初始化 + 条目与历史版本的读取。
//!
//! 与 Go 版 db.go 对齐：
//! - 打开时启用 WAL 与外键；
//! - 表结构：meta / secret_items / secret_versions / settings；
//! - 盐值存于 meta 表 key='salt'，base64 编码。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::{Local, SecondsFormat};
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};

use crate::crypto::{self, CryptoError};

/// 当前时间，RFC3339 秒精度，与 Go 版 nowISO() 格式一致。
fn now_iso() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

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

    /// 当前盐值（base64，未设置主密码时返回 None）。
    pub fn salt_b64(&self) -> Option<String> {
        self.salt.as_ref().map(|s| BASE64.encode(s))
    }

    /// 把 WAL 日志合并进主数据库文件（供外部复制单个 .db 文件前调用）。
    pub fn checkpoint_wal(&self) -> Result<(), SecretboxError> {
        // PRAGMA 会返回一行统计结果，用 query_row 执行
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))?;
        Ok(())
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

    /// 首次设置主密码：生成随机盐、写 meta 表并更新内存盐值，返回派生密钥。
    pub fn setup_master_password(&mut self, password: &str) -> Result<Vec<u8>, SecretboxError> {
        let (key, salt) = crypto::derive_key(password, &[])?;
        self.conn.execute(
            "INSERT INTO meta(key,value) VALUES('salt',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [&BASE64.encode(&salt)],
        )?;
        self.salt = Some(salt);
        Ok(key)
    }

    /// 读取设置项，不存在返回默认值（与 Go 版 GetSetting 一致）。
    pub fn get_setting(&self, key: &str, default: &str) -> Result<String, SecretboxError> {
        match self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            }) {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(default.to_string()),
            Err(err) => Err(err.into()),
        }
    }

    /// 写入设置项。
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), SecretboxError> {
        self.conn.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// 读取全部设置（键值对）。
    pub fn get_all_settings(&self) -> Result<Vec<(String, String)>, SecretboxError> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM settings")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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

    /// 读取单条的元数据与密文（事务内版本，供写路径复用）。
    fn query_item_in_tx(tx: &Transaction, id: i64) -> Result<(Item, String), SecretboxError> {
        tx.query_row(
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

    // ---------- 写路径（与 Go 版 CreateItem/UpdateItem/DeleteItem 对齐） ----------

    /// 新增条目并写入首个历史版本，返回新条目 ID。
    pub fn create_item(
        &self,
        key: &[u8],
        title: &str,
        category: &str,
        note: &str,
        value: &str,
    ) -> Result<i64, SecretboxError> {
        let encrypted = crypto::encrypt(key, value)?;
        let now = now_iso();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO secret_items(title,category,note,encrypted_value,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?5)",
            rusqlite::params![title, category, note, encrypted, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO secret_versions(secret_id,version,encrypted_snapshot,created_at)
             VALUES(?1,1,?2,?3)",
            rusqlite::params![id, encrypted, now],
        )?;
        tx.commit()?;
        Ok(id)
    }

    /// 更新条目标题/分类/备注/内容，并创建新历史版本，返回解密后的最新条目。
    pub fn update_item(
        &self,
        key: &[u8],
        id: i64,
        title: &str,
        category: &str,
        note: &str,
        value: &str,
    ) -> Result<Item, SecretboxError> {
        let encrypted = crypto::encrypt(key, value)?;
        let now = now_iso();
        let tx = self.conn.unchecked_transaction()?;

        // 计算下一版本号
        let next_version: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM secret_versions WHERE secret_id = ?1",
            [id],
            |row| row.get(0),
        )?;
        tx.execute(
            "INSERT INTO secret_versions(secret_id,version,encrypted_snapshot,created_at)
             VALUES(?1,?2,?3,?4)",
            rusqlite::params![id, next_version, encrypted, now],
        )?;
        let affected = tx.execute(
            "UPDATE secret_items SET title=?1, category=?2, note=?3, encrypted_value=?4, updated_at=?5
             WHERE id=?6",
            rusqlite::params![title, category, note, encrypted, now, id],
        )?;
        if affected == 0 {
            return Err(SecretboxError::ItemNotFound);
        }
        let (mut item, item_encrypted) = Self::query_item_in_tx(&tx, id)?;
        item.value = crypto::decrypt(key, &item_encrypted)?;
        tx.commit()?;
        Ok(item)
    }

    /// 删除条目（历史版本因 ON DELETE CASCADE 一并删除）。
    pub fn delete_item(&self, id: i64) -> Result<(), SecretboxError> {
        self.conn
            .execute("DELETE FROM secret_items WHERE id = ?1", [id])?;
        Ok(())
    }

    /// 删除指定历史版本（与 Go 版 DeleteVersion 一致，不存在时报错）。
    pub fn delete_version(&self, secret_id: i64, version: i64) -> Result<(), SecretboxError> {
        let affected = self.conn.execute(
            "DELETE FROM secret_versions WHERE secret_id = ?1 AND version = ?2",
            [secret_id, version],
        )?;
        if affected == 0 {
            return Err(SecretboxError::VersionNotFound);
        }
        Ok(())
    }

    /// 修改主密码：全部条目与历史版本用新密码重新加密，更新 meta 盐值
    /// （与 Go 版 ChangePassword 一致：新随机盐 + 事务内重加密 + 更新 meta）。
    /// `old_key` 为旧密码派生的当前会话密钥；任一密文解密失败即整体回滚。
    /// 返回新派生密钥，调用方应更新会话密钥。
    pub fn change_password(
        &mut self,
        old_key: &[u8],
        new_password: &str,
    ) -> Result<Vec<u8>, SecretboxError> {
        // 先读出全部密文（条目 + 历史版本），避免事务内遍历与更新互相干扰
        let mut item_rows: Vec<(i64, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, encrypted_value FROM secret_items")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let version_rows: Vec<(i64, String)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, encrypted_snapshot FROM secret_versions")?;
            let rows = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };

        // 用新随机盐派生新密钥
        let (new_key, new_salt) = crypto::derive_key(new_password, &[])?;

        let tx = self.conn.unchecked_transaction()?;
        for (id, enc) in &item_rows {
            let plain = crypto::decrypt(old_key, enc)?;
            let re_encrypted = crypto::encrypt(&new_key, &plain)?;
            tx.execute(
                "UPDATE secret_items SET encrypted_value = ?1 WHERE id = ?2",
                rusqlite::params![re_encrypted, id],
            )?;
        }
        for (id, enc) in &version_rows {
            let plain = crypto::decrypt(old_key, enc)?;
            let re_encrypted = crypto::encrypt(&new_key, &plain)?;
            tx.execute(
                "UPDATE secret_versions SET encrypted_snapshot = ?1 WHERE id = ?2",
                rusqlite::params![re_encrypted, id],
            )?;
        }
        // 更新盐值
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('salt',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [&BASE64.encode(&new_salt)],
        )?;
        tx.commit()?;

        self.salt = Some(new_salt);
        Ok(new_key)
    }

    /// 恢复历史版本：把指定版本的快照内容作为新修改写入条目（与 Go 版 handleRestore 一致，
    /// 会产生一个新版本记录）。保留条目当前的标题/分类/备注，返回恢复后的最新条目。
    pub fn restore_version(
        &self,
        key: &[u8],
        secret_id: i64,
        version: i64,
    ) -> Result<Item, SecretboxError> {
        let content = self.get_version_snapshot(key, secret_id, version)?;
        let item = self.get_item(key, secret_id)?;
        self.update_item(key, secret_id, &item.title, &item.category, &item.note, &content)
    }
}
