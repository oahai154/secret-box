//! 数据层：SQLite 打开/初始化 + 条目与历史版本的读取。
//!
//! v2 存储格式（见 ADR-0003）：
//! - 条目与历史版本由随机 DEK（数据加密密钥）加解密；
//! - meta 表 key='salt' 存 KEK 盐值（base64），key='wrapped_dek' 存
//!   主密码派生 KEK 包装后的 DEK（base64）；
//! - 解锁 = 用主密码解开 DEK 包装，GCM 标签即密码校验；
//! - 无 wrapped_dek 的旧版（v1）数据库为只读导入源：解锁退回
//!   主密码直接派生路径，仅供升级向导与旧快照导入读取。
//!
//! WAL 与外键、表结构与 Go 版一致。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::{Local, SecondsFormat};
use rusqlite::{Connection, Transaction};
use serde::{Deserialize, Serialize};

use crate::crypto::{self, CryptoError, KEY_LEN};
use crate::migration::{Snapshot, SnapshotItem, SnapshotVersion};
use crate::recovery::{format_grouped, normalize_recovery_key};

/// meta 表中 KEK 盐值的键。
const META_SALT: &str = "salt";
/// meta 表中主密码 KEK 包装后的 DEK（base64）的键。
const META_WRAPPED_DEK: &str = "wrapped_dek";
/// meta 表中恢复密钥 KEK 盐值的键。
const META_RECOVERY_SALT: &str = "recovery_salt";
/// meta 表中恢复密钥 KEK 包装后的 DEK（base64）的键。
const META_WRAPPED_DEK_RECOVERY: &str = "wrapped_dek_recovery";
/// meta 表中 DEK 加密的恢复密钥明文（base64）的键，供解锁后查看。
const META_RECOVERY_KEY_ENC: &str = "recovery_key_enc";

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
    #[error("迁移文件格式无效")]
    MigrationFormatInvalid,
    #[error("迁移口令错误或文件已损坏")]
    MigrationPassphraseWrong,
    #[error("迁移文件内容无效")]
    MigrationContentInvalid,
    #[error("序列化快照失败")]
    SnapshotSerializeFailed,
    #[error("删除数据库文件失败: {0}")]
    RemoveDbFailed(String),
    #[error("旧版数据库为只读导入源，请先完成升级")]
    LegacyReadOnly,
    #[error("恢复密钥未设置")]
    RecoveryNotSet,
    #[error("恢复密钥不正确")]
    RecoveryWrong,
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

    /// 是否为 v2 格式（存在 DEK 包装行）。
    pub fn is_v2(&self) -> bool {
        self.read_wrapped_dek().ok().flatten().is_some()
    }

    /// 读取 meta 表中包装的 DEK（base64 原文；不存在返回 None）。
    fn read_wrapped_dek(&self) -> Result<Option<String>, SecretboxError> {
        match self.conn.query_row(
            "SELECT value FROM meta WHERE key='wrapped_dek'",
            [],
            |row| row.get::<_, String>(0),
        ) {
            Ok(encoded) => Ok(Some(encoded)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn write_meta(&self, tx: &Transaction, key: &str, value: &str) -> Result<(), SecretboxError> {
        tx.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// 首次设置主密码：生成随机 DEK 与盐值，DEK 用主密码派生的 KEK 包装后
    /// 连同盐值写入 meta 表，返回 DEK（只应存于内存，作为会话密钥）。
    /// 已设置主密码的库（无论 v1/v2）拒绝再次设置——存量 v1 库只能走升级
    /// 向导迁移，不存在任何覆盖式写入口（见 ADR-0003）。
    pub fn setup_master_password(&mut self, password: &str) -> Result<Vec<u8>, SecretboxError> {
        if self.has_master_password() {
            return Err(SecretboxError::LegacyReadOnly);
        }
        let dek = crypto::random_bytes(KEY_LEN)?;
        let (kek, salt) = crypto::derive_key(password, &[])?;
        let wrapped = crypto::encrypt_bytes(&kek, &dek)?;
        let tx = self.conn.unchecked_transaction()?;
        self.write_meta(&tx, META_SALT, &BASE64.encode(&salt))?;
        self.write_meta(&tx, META_WRAPPED_DEK, &wrapped)?;
        tx.commit()?;
        self.salt = Some(salt);
        Ok(dek)
    }

    /// 是否已设置恢复密钥（依据恢复侧包装是否已写）。
    pub fn has_recovery_key(&self) -> bool {
        self.read_meta(META_WRAPPED_DEK_RECOVERY)
            .map(|v| v.is_some())
            .unwrap_or(false)
    }

    /// 设置（或重置）恢复密钥：归一化后派生恢复侧 KEK（独立随机盐），
    /// 重新包装 DEK 写入 meta；同时把规范分组的明文码用 DEK 加密存一份
    /// （解锁后可查看，锁定态无 DEK 不可见）。旧恢复密钥随之作废。
    /// `dek` 为当前会话密钥；v1 旧库没有 DEK，拒绝设置。
    pub fn set_recovery_key(
        &mut self,
        recovery_key: &str,
        dek: &[u8],
    ) -> Result<(), SecretboxError> {
        if self.read_wrapped_dek()?.is_none() {
            return Err(SecretboxError::LegacyReadOnly);
        }
        let normalized = normalize_recovery_key(recovery_key);
        if normalized.len() < 16 {
            return Err(SecretboxError::Crypto(CryptoError::DeriveFailed));
        }
        let canonical = format_grouped(&normalized);
        let (kek, salt) = crypto::derive_key(&normalized, &[])?;
        let wrapped = crypto::encrypt_bytes(&kek, dek)?;
        let code_enc = crypto::encrypt(dek, &canonical)?;
        let tx = self.conn.unchecked_transaction()?;
        self.write_meta(&tx, META_RECOVERY_SALT, &BASE64.encode(&salt))?;
        self.write_meta(&tx, META_WRAPPED_DEK_RECOVERY, &wrapped)?;
        self.write_meta(&tx, META_RECOVERY_KEY_ENC, &code_enc)?;
        tx.commit()?;
        Ok(())
    }

    /// 查看（解锁态）当前恢复密钥，返回与生成/重置时一致的规范分组码。
    /// 锁定状态下调用方没有 DEK，无法获取——这是"锁定不暴露"的机制保证。
    pub fn get_recovery_key(&self, dek: &[u8]) -> Result<Option<String>, SecretboxError> {
        match self.read_meta(META_RECOVERY_KEY_ENC)? {
            Some(encoded) => crypto::decrypt(dek, &encoded).map(Some).map_err(|_| {
                SecretboxError::Crypto(CryptoError::DecryptFailed)
            }),
            None => Ok(None),
        }
    }

    /// 用恢复密钥解开 DEK 包装（忘记主密码的救援路径，见 ADR-0003）。
    /// 成功返回 DEK，与主密码解锁等价。
    pub fn unlock_with_recovery_key(&self, recovery_key: &str) -> Result<Vec<u8>, SecretboxError> {
        let wrapped = self
            .read_meta(META_WRAPPED_DEK_RECOVERY)?
            .ok_or(SecretboxError::RecoveryNotSet)?;
        let salt_b64 = self
            .read_meta(META_RECOVERY_SALT)?
            .ok_or(SecretboxError::RecoveryNotSet)?;
        let salt = BASE64
            .decode(&salt_b64)
            .map_err(|_| SecretboxError::Base64DecodeFailed)?;
        let normalized = normalize_recovery_key(recovery_key);
        let (kek, _) = crypto::derive_key(&normalized, &salt)?;
        crypto::decrypt_bytes(&kek, &wrapped).map_err(|_| SecretboxError::RecoveryWrong)
    }

    /// 读取 meta 表某键的原始值。
    fn read_meta(&self, key: &str) -> Result<Option<String>, SecretboxError> {
        match self.conn.query_row(
            "SELECT value FROM meta WHERE key=?1",
            [key],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
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

    /// 用主密码解锁：解开 DEK 包装（GCM 标签即密码校验），成功返回 DEK
    /// （只应存于内存）。无包装行的 v1 旧库退回主密码直接派生路径
    /// （只读导入源，供升级向导与旧快照导入读取）。
    pub fn unlock(&self, password: &str) -> Result<Vec<u8>, SecretboxError> {
        if let Some(wrapped) = self.read_wrapped_dek()? {
            let salt = self
                .salt
                .as_ref()
                .ok_or(SecretboxError::Crypto(CryptoError::DeriveFailed))?;
            let (kek, _) = crypto::derive_key(password, salt)?;
            return Ok(crypto::decrypt_bytes(&kek, &wrapped)?);
        }

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

    /// 返回全部条目并解密各自 value（不含历史版本），供明文导出使用（ADR-0004）。
    /// 仅在解锁态调用；锁定态由调用方（require_unlocked）拒绝。
    pub fn list_items_with_values(&self, key: &[u8]) -> Result<Vec<Item>, SecretboxError> {
        self.list_items()?
            .into_iter()
            .map(|mut it| {
                let (_, encrypted) = self.query_item(it.id)?;
                it.value = crypto::decrypt(key, &encrypted)?;
                Ok(it)
            })
            .collect()
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

    /// 修改主密码：生成新盐值与新 KEK，仅重新包装 DEK，条目密文原样不动
    /// （v2 起"改密"不再触发全库重加密）。`dek` 为当前会话密钥（解锁时
    /// 解包装得到）；v1 旧库没有 DEK，拒绝修改（升级向导负责迁移）。
    /// 返回会话密钥（即原 DEK，保持不变）。
    pub fn change_password(
        &mut self,
        dek: &[u8],
        new_password: &str,
    ) -> Result<Vec<u8>, SecretboxError> {
        if self.read_wrapped_dek()?.is_none() {
            return Err(SecretboxError::LegacyReadOnly);
        }
        let (new_kek, new_salt) = crypto::derive_key(new_password, &[])?;
        let wrapped = crypto::encrypt_bytes(&new_kek, dek)?;
        let tx = self.conn.unchecked_transaction()?;
        self.write_meta(&tx, META_SALT, &BASE64.encode(&new_salt))?;
        self.write_meta(&tx, META_WRAPPED_DEK, &wrapped)?;
        tx.commit()?;
        self.salt = Some(new_salt);
        Ok(dek.to_vec())
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

    // ---------- 快照导出 / 导入 / 清除痕迹（与 Go 版 GetSnapshot/RestoreFromSnapshot/Wipe 对齐） ----------

    /// 读取当前数据库的完整数据快照（条目密文 + 全部密钥包装材料，不解密）。
    /// v2 携带主密码侧与恢复密钥侧两份包装——备份不随主密码遗忘而作废。
    pub fn get_snapshot(&self) -> Result<Snapshot, SecretboxError> {
        // 随快照透传的 meta 键：主密码侧 + 恢复密钥侧包装材料
        const META_KEYS: [&str; 5] = [
            META_SALT,
            META_WRAPPED_DEK,
            META_RECOVERY_SALT,
            META_WRAPPED_DEK_RECOVERY,
            META_RECOVERY_KEY_ENC,
        ];
        let mut snap = Snapshot {
            has_password: self.has_master_password(),
            salt_b64: String::new(),
            wrapped_dek_b64: String::new(),
            recovery_salt_b64: String::new(),
            wrapped_dek_recovery_b64: String::new(),
            recovery_key_enc_b64: String::new(),
            items: Vec::new(),
        };
        for key in META_KEYS {
            if let Some(value) = self.read_meta(key)? {
                match key {
                    META_SALT => snap.salt_b64 = value,
                    META_WRAPPED_DEK => snap.wrapped_dek_b64 = value,
                    META_RECOVERY_SALT => snap.recovery_salt_b64 = value,
                    META_WRAPPED_DEK_RECOVERY => snap.wrapped_dek_recovery_b64 = value,
                    META_RECOVERY_KEY_ENC => snap.recovery_key_enc_b64 = value,
                    _ => unreachable!("META_KEYS 与字段映射同步维护"),
                }
            }
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, title, category, note, encrypted_value, created_at, updated_at
             FROM secret_items ORDER BY id",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let mut item = SnapshotItem {
                title: row.get(1)?,
                category: row.get(2)?,
                note: row.get(3)?,
                value: row.get(4)?,
                created: row.get(5)?,
                updated: row.get(6)?,
                versions: Vec::new(),
            };
            let mut vstmt = self.conn.prepare(
                "SELECT version, encrypted_snapshot, created_at
                 FROM secret_versions WHERE secret_id = ?1 ORDER BY version",
            )?;
            let mut vrows = vstmt.query([id])?;
            while let Some(vrow) = vrows.next()? {
                item.versions.push(SnapshotVersion {
                    version: vrow.get(0)?,
                    snapshot: vrow.get(1)?,
                    created: vrow.get(2)?,
                });
            }
            snap.items.push(item);
        }
        Ok(snap)
    }

    /// 用给定快照重建整个库（先清空再写入），并重读盐值。
    pub fn restore_from_snapshot(&mut self, snap: &Snapshot) -> Result<(), SecretboxError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM secret_versions", [])?;
        tx.execute("DELETE FROM secret_items", [])?;
        tx.execute("DELETE FROM meta", [])?;
        if !snap.salt_b64.is_empty() {
            tx.execute(
                "INSERT INTO meta(key,value) VALUES('salt',?1)",
                [&snap.salt_b64],
            )?;
        }
        if !snap.wrapped_dek_b64.is_empty() {
            tx.execute(
                "INSERT INTO meta(key,value) VALUES('wrapped_dek',?1)",
                [&snap.wrapped_dek_b64],
            )?;
        }
        if !snap.recovery_salt_b64.is_empty() {
            tx.execute(
                "INSERT INTO meta(key,value) VALUES('recovery_salt',?1)",
                [&snap.recovery_salt_b64],
            )?;
        }
        if !snap.wrapped_dek_recovery_b64.is_empty() {
            tx.execute(
                "INSERT INTO meta(key,value) VALUES('wrapped_dek_recovery',?1)",
                [&snap.wrapped_dek_recovery_b64],
            )?;
        }
        if !snap.recovery_key_enc_b64.is_empty() {
            tx.execute(
                "INSERT INTO meta(key,value) VALUES('recovery_key_enc',?1)",
                [&snap.recovery_key_enc_b64],
            )?;
        }
        for it in &snap.items {
            if it.value.is_empty() {
                continue;
            }
            tx.execute(
                "INSERT INTO secret_items(title,category,note,encrypted_value,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params![it.title, it.category, it.note, it.value, it.created, it.updated],
            )?;
            let id = tx.last_insert_rowid();
            for v in &it.versions {
                if v.snapshot.is_empty() {
                    continue;
                }
                tx.execute(
                    "INSERT INTO secret_versions(secret_id,version,encrypted_snapshot,created_at)
                     VALUES(?1,?2,?3,?4)",
                    rusqlite::params![id, v.version, v.snapshot, v.created],
                )?;
            }
        }
        tx.commit()?;
        self.reload_salt()
    }

    /// 清除全部数据（条目、版本与盐值/主密码设置），然后关闭连接并删除数据库文件
    /// （含 -wal/-shm），实现"导出后清除本地痕迹"。本方法消耗自身。
    pub fn wipe_and_remove_files(self) -> Result<(), SecretboxError> {
        let path = self.db_path.clone();
        {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute("DELETE FROM secret_versions", [])?;
            tx.execute("DELETE FROM secret_items", [])?;
            tx.execute("DELETE FROM meta", [])?;
            tx.commit()?;
        }
        drop(self.conn);

        // Windows 杀毒可能瞬时锁文件，重试删除
        let mut last_err = None;
        for _ in 0..5 {
            let gone = ["" , "-wal", "-shm"].iter().all(|suffix| {
                !std::path::Path::new(&format!("{path}{suffix}")).exists()
            });
            if gone {
                return Ok(());
            }
            last_err = std::fs::remove_file(&path)
                .and_then(|_| maybe_remove(&format!("{path}-wal")))
                .and_then(|_| maybe_remove(&format!("{path}-shm")))
                .err();
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Err(SecretboxError::RemoveDbFailed(
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "文件仍存在".to_string()),
        ))
    }
}

fn maybe_remove(path: &str) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        // 文件本就不存在视为成功
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}
