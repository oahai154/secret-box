// lib.rs - Tauri 应用层：把 secretbox-core 暴露为 IPC 命令。
// 安全敏感逻辑（加解密、存储）都在 secretbox-core，这里只做参数搬运。
//
// rusqlite 的连接不是 Sync，因此数据库句柄与内存密钥都放进 Mutex。
// 命令拆成 *_impl 纯函数，便于单元测试直接调用。

use std::collections::BTreeMap;
use std::sync::Mutex;

use secretbox_core::{Db, Item, Version};
use tauri::State;

/// 应用运行时状态：数据库句柄 + 解锁后的派生密钥（仅存内存）。
pub struct AppState {
    db: Mutex<Option<Db>>,
    key: Mutex<Option<Vec<u8>>>,
}

/// 启动 Tauri 应用。数据库打开失败时返回错误，由入口决定如何报告。
pub fn run(db_path: &str) -> Result<(), String> {
    let state = open_state(db_path)?;
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            unlock,
            lock,
            setup_password,
            verify_password,
            list_items,
            get_item,
            create_item,
            update_item,
            delete_item,
            list_versions,
            restore_version,
            delete_version,
            get_version_snapshot,
            get_settings,
            update_settings
        ])
        .run(tauri::generate_context!())
        .map_err(|err| format!("Tauri 应用运行异常: {err}"))?;
    Ok(())
}

fn open_state(db_path: &str) -> Result<AppState, String> {
    let db = Db::open(db_path).map_err(|err| format!("数据库初始化失败: {err}"))?;
    Ok(AppState {
        db: Mutex::new(Some(db)),
        key: Mutex::new(None),
    })
}

// ---------- 命令实现（独立于 Tauri 宏，可测试） ----------

/// 状态：是否已设置主密码、当前是否已解锁。
fn status_impl(state: &AppState) -> Result<serde_json::Value, String> {
    with_db(state, |db| {
        Ok(serde_json::json!({
            "has_password": db.has_master_password(),
            "unlocked": current_key(state).is_some(),
        }))
    })
}

/// 解锁：用主密码派生密钥并验证，成功后密钥只存内存。
fn unlock_impl(state: &AppState, password: &str) -> Result<serde_json::Value, String> {
    let key = with_db(state, |db| db.unlock(password).map_err(|err| err.to_string()))?;
    let key_len = key.len();
    *state.key.lock().expect("密钥锁不可中毒") = Some(key);
    Ok(serde_json::json!({ "key_len": key_len }))
}

/// 锁定：清空内存密钥；明文从不落盘，也随密钥一起不可再取。
fn lock_impl(state: &AppState) -> Result<serde_json::Value, String> {
    *state.key.lock().expect("密钥锁不可中毒") = None;
    Ok(serde_json::json!({ "locked": true }))
}

/// 首次设置主密码。
fn setup_password_impl(state: &AppState, password: &str) -> Result<serde_json::Value, String> {
    let trimmed = password.trim();
    if trimmed.chars().count() < 4 {
        return Err("主密码至少 4 个字符".to_string());
    }
    let has_password = with_db(state, |db| Ok(db.has_master_password()))?;
    if has_password {
        return Err("主密码已设置".to_string());
    }
    let key = with_db(state, |db| {
        db.setup_master_password(trimmed)
            .map_err(|err| err.to_string())
    })?;
    *state.key.lock().expect("密钥锁不可中毒") = Some(key);
    Ok(serde_json::json!({ "ok": true }))
}

/// 列出全部条目（不含明文）。未解锁时拒绝。
fn list_items_impl(state: &AppState) -> Result<Vec<Item>, String> {
    require_unlocked(state)?;
    with_db(state, |db| db.list_items().map_err(|err| err.to_string()))
}

/// 读取单条并返回解密明文。未解锁时拒绝。
fn get_item_impl(state: &AppState, id: i64) -> Result<Item, String> {
    let key = require_unlocked(state)?;
    with_db(state, |db| db.get_item(&key, id).map_err(|err| err.to_string()))
}

/// 某条目的全部历史版本（仅元数据）。未解锁时拒绝。
fn list_versions_impl(state: &AppState, id: i64) -> Result<Vec<Version>, String> {
    require_unlocked(state)?;
    with_db(state, |db| db.list_versions(id).map_err(|err| err.to_string()))
}

/// 校验主密码（用于删除等敏感操作的确认），不改变当前会话。
fn verify_password_impl(state: &AppState, password: &str) -> Result<(), String> {
    with_db(state, |db| {
        db.unlock(password).map(|_| ()).map_err(|err| err.to_string())
    })
}

/// 新增条目，返回新 ID。未解锁时拒绝。
fn create_item_impl(
    state: &AppState,
    title: &str,
    category: &str,
    note: &str,
    value: &str,
) -> Result<i64, String> {
    let key = require_unlocked(state)?;
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("标题不能为空".to_string());
    }
    with_db(state, |db| {
        db.create_item(&key, trimmed, category, note, value)
            .map_err(|err| err.to_string())
    })
}

/// 更新条目，返回解密后的最新条目。未解锁时拒绝。
#[allow(clippy::too_many_arguments)]
fn update_item_impl(
    state: &AppState,
    id: i64,
    title: &str,
    category: &str,
    note: &str,
    value: &str,
) -> Result<Item, String> {
    let key = require_unlocked(state)?;
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("标题不能为空".to_string());
    }
    with_db(state, |db| {
        db.update_item(&key, id, trimmed, category, note, value)
            .map_err(|err| err.to_string())
    })
}

/// 删除条目（历史版本一并删除）。未解锁时拒绝。
fn delete_item_impl(state: &AppState, id: i64) -> Result<(), String> {
    let _key = require_unlocked(state)?;
    with_db(state, |db| {
        db.delete_item(id).map_err(|err| err.to_string())
    })
}

/// 恢复历史版本（快照内容作为新修改写入，产生新版本记录）。未解锁时拒绝。
fn restore_version_impl(state: &AppState, id: i64, version: i64) -> Result<Item, String> {
    let key = require_unlocked(state)?;
    with_db(state, |db| {
        db.restore_version(&key, id, version)
            .map_err(|err| err.to_string())
    })
}

/// 删除历史版本。未解锁时拒绝。
fn delete_version_impl(state: &AppState, id: i64, version: i64) -> Result<(), String> {
    let _key = require_unlocked(state)?;
    with_db(state, |db| {
        db.delete_version(id, version)
            .map_err(|err| err.to_string())
    })
}

/// 读取某历史版本的快照明文（用于"查看历史版本"）。未解锁时拒绝。
fn get_version_snapshot_impl(state: &AppState, id: i64, version: i64) -> Result<String, String> {
    let key = require_unlocked(state)?;
    with_db(state, |db| {
        db.get_version_snapshot(&key, id, version)
            .map_err(|err| err.to_string())
    })
}

/// 写入设置项。
fn update_settings_impl(state: &AppState, settings: &BTreeMap<String, String>) -> Result<(), String> {
    with_db(state, |db| {
        for (key, value) in settings {
            db.set_setting(key, value)
                .map_err(|_err| format!("保存设置失败: {key}"))?;
        }
        Ok(())
    })
}

/// 读取全部设置，补充默认值（与 Go 版 handleGetSettings 一致）。
fn get_settings_impl(state: &AppState) -> Result<BTreeMap<String, String>, String> {
    with_db(state, |db| {
        let mut result = BTreeMap::new();
        for (key, value) in db.get_all_settings().map_err(|err| err.to_string())? {
            result.insert(key, value);
        }
        result
            .entry("auto_lock_seconds".to_string())
            .or_insert_with(|| "120".to_string());
        result
            .entry("delete_requires_password".to_string())
            .or_insert_with(|| "true".to_string());
        result
            .entry("delete_version_requires_password".to_string())
            .or_insert_with(|| "true".to_string());
        Ok(result)
    })
}

fn require_unlocked(state: &AppState) -> Result<Vec<u8>, String> {
    current_key(state).ok_or_else(|| "未解锁".to_string())
}

fn current_key(state: &AppState) -> Option<Vec<u8>> {
    state.key.lock().expect("密钥锁不可中毒").clone()
}

fn with_db<T>(state: &AppState, f: impl FnOnce(&mut Db) -> Result<T, String>) -> Result<T, String> {
    let mut guard = state
        .db
        .lock()
        .map_err(|_| "数据库句柄锁异常".to_string())?;
    let db = guard
        .as_mut()
        .ok_or_else(|| "数据库未打开".to_string())?;
    f(db)
}

// ---------- Tauri 命令包装 ----------

#[tauri::command]
fn get_status(state: State<AppState>) -> Result<serde_json::Value, String> {
    status_impl(&state)
}

#[tauri::command]
fn unlock(state: State<AppState>, password: String) -> Result<serde_json::Value, String> {
    unlock_impl(&state, &password)
}

#[tauri::command]
fn lock(state: State<AppState>) -> Result<serde_json::Value, String> {
    lock_impl(&state)
}

#[tauri::command]
fn setup_password(state: State<AppState>, password: String) -> Result<serde_json::Value, String> {
    setup_password_impl(&state, &password)
}

#[tauri::command]
fn list_items(state: State<AppState>) -> Result<Vec<Item>, String> {
    list_items_impl(&state)
}

#[tauri::command]
fn get_item(state: State<AppState>, id: i64) -> Result<Item, String> {
    get_item_impl(&state, id)
}

#[tauri::command]
fn list_versions(state: State<AppState>, id: i64) -> Result<Vec<Version>, String> {
    list_versions_impl(&state, id)
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Result<BTreeMap<String, String>, String> {
    get_settings_impl(&state)
}

#[tauri::command]
fn verify_password(state: State<AppState>, password: String) -> Result<(), String> {
    verify_password_impl(&state, &password)
}

#[tauri::command]
fn create_item(
    state: State<AppState>,
    title: String,
    category: String,
    note: String,
    value: String,
) -> Result<i64, String> {
    create_item_impl(&state, &title, &category, &note, &value)
}

#[tauri::command]
fn update_item(
    state: State<AppState>,
    id: i64,
    title: String,
    category: String,
    note: String,
    value: String,
) -> Result<Item, String> {
    update_item_impl(&state, id, &title, &category, &note, &value)
}

#[tauri::command]
fn delete_item(state: State<AppState>, id: i64) -> Result<(), String> {
    delete_item_impl(&state, id)
}

#[tauri::command]
fn restore_version(state: State<AppState>, id: i64, version: i64) -> Result<Item, String> {
    restore_version_impl(&state, id, version)
}

#[tauri::command]
fn delete_version(state: State<AppState>, id: i64, version: i64) -> Result<(), String> {
    delete_version_impl(&state, id, version)
}

#[tauri::command]
fn get_version_snapshot(state: State<AppState>, id: i64, version: i64) -> Result<String, String> {
    get_version_snapshot_impl(&state, id, version)
}

#[tauri::command]
fn update_settings(
    state: State<AppState>,
    settings: BTreeMap<String, String>,
) -> Result<(), String> {
    update_settings_impl(&state, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 打开黄金样本副本的测试状态（复制到临时目录，避免污染 fixture）。
    fn open_test_state() -> (AppState, std::path::PathBuf) {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/secretbox-core/tests/fixtures/golden.db"
        );
        let tmp = std::env::temp_dir().join(format!(
            "secretbox-lib-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let db_copy = tmp.join("golden.db");
        // Windows 上杀毒软件可能瞬时锁住新写入的文件，重试几次
        let mut copied = false;
        for _ in 0..5 {
            match std::fs::copy(fixture, &db_copy) {
                Ok(_) => {
                    copied = true;
                    break;
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
            }
        }
        assert!(copied, "复制 fixture 失败");
        (open_state(db_copy.to_str().unwrap()).unwrap(), tmp)
    }

    #[test]
    fn 解锁后可读条目_锁定后明文不可再取() {
        let (state, _tmp) = open_test_state();

        // 锁定状态下读取被拒绝
        assert_eq!(list_items_impl(&state).unwrap_err(), "未解锁");

        unlock_impl(&state, "golden-test-password").unwrap();
        let items = list_items_impl(&state).unwrap();
        assert_eq!(items.len(), 3);

        // 解锁状态下可以读到明文
        let detail = get_item_impl(&state, items[0].id).unwrap();
        assert!(!detail.value.is_empty());

        // 锁定后：密钥清空、明文不可再取
        lock_impl(&state).unwrap();
        assert!(current_key(&state).is_none(), "锁定后密钥必须清空");
        assert_eq!(list_items_impl(&state).unwrap_err(), "未解锁");
        assert_eq!(get_item_impl(&state, items[0].id).unwrap_err(), "未解锁");
    }

    #[test]
    fn 错误密码解锁失败() {
        let (state, _tmp) = open_test_state();
        let err = unlock_impl(&state, "绝对错误的密码").unwrap_err();
        assert!(err.contains("主密码可能不正确"));
        assert!(current_key(&state).is_none());
    }

    #[test]
    fn 读取设置默认值() {
        let (state, _tmp) = open_test_state();
        let settings = get_settings_impl(&state).unwrap();
        assert_eq!(settings.get("auto_lock_seconds").unwrap(), "120");
        assert_eq!(settings.get("delete_requires_password").unwrap(), "true");
    }

    #[test]
    fn 已设主密码的库不允许再次设置() {
        let (state, _tmp) = open_test_state();
        assert!(setup_password_impl(&state, "new-password").is_err());
    }
}
