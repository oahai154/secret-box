// lib.rs - Tauri 应用层：把 secretbox-core 暴露为 IPC 命令。
// 安全敏感逻辑（加解密、存储）都在 secretbox-core，这里只做参数搬运。
//
// rusqlite 的连接不是 Sync，因此数据库句柄与内存密钥都放进 Mutex。

use std::sync::Mutex;

use secretbox_core::{Db, Item};
use tauri::State;

/// 应用运行时状态：数据库句柄 + 解锁后的派生密钥（仅存内存）。
pub struct AppState {
    db: Mutex<Option<Db>>,
    key: Mutex<Option<Vec<u8>>>,
}

/// 启动 Tauri 应用。数据库打开失败时返回错误，由入口决定如何报告。
pub fn run(db_path: &str) -> Result<(), String> {
    let db = Db::open(db_path).map_err(|err| format!("数据库初始化失败: {err}"))?;
    tauri::Builder::default()
        .manage(AppState {
            db: Mutex::new(Some(db)),
            key: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![get_status, unlock, list_items])
        .run(tauri::generate_context!())
        .map_err(|err| format!("Tauri 应用运行异常: {err}"))?;
    Ok(())
}

/// 状态：是否已设置主密码、当前是否已解锁。
#[tauri::command]
fn get_status(state: State<AppState>) -> Result<serde_json::Value, String> {
    with_db(&state, |db| {
        let unlocked = current_key(&state).is_some();
        Ok(serde_json::json!({
            "has_password": db.has_master_password(),
            "unlocked": unlocked,
        }))
    })
}

/// 解锁：用主密码派生密钥并验证，成功后密钥只存内存。
#[tauri::command]
fn unlock(state: State<AppState>, password: String) -> Result<serde_json::Value, String> {
    let key = with_db(&state, |db| {
        db.unlock(&password).map_err(|err| err.to_string())
    })?;
    let key_len = key.len();
    *state.key.lock().expect("密钥锁不可中毒") = Some(key);
    Ok(serde_json::json!({ "key_len": key_len }))
}

/// 列出全部条目（不含明文）。未解锁时拒绝。
#[tauri::command]
fn list_items(state: State<AppState>) -> Result<Vec<Item>, String> {
    if current_key(&state).is_none() {
        return Err("未解锁".to_string());
    }
    with_db(&state, |db| db.list_items().map_err(|err| err.to_string()))
}

/// 借出数据库句柄执行一次操作（未打开时报错）。
fn with_db<T>(
    state: &State<AppState>,
    f: impl FnOnce(&Db) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state
        .db
        .lock()
        .map_err(|_| "数据库句柄锁异常".to_string())?;
    let db = guard.as_ref().ok_or_else(|| "数据库未打开".to_string())?;
    f(db)
}

fn current_key(state: &State<AppState>) -> Option<Vec<u8>> {
    state
        .key
        .lock()
        .expect("密钥锁不可中毒")
        .clone()
}
