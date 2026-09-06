// lib.rs - Tauri 应用层：把 secretbox-core 暴露为 IPC 命令。
// 安全敏感逻辑（加解密、存储）都在 secretbox-core，这里只做参数搬运。
//
// rusqlite 的连接不是 Sync，因此数据库句柄与内存密钥都放进 Mutex。
// 命令拆成 *_impl 纯函数，便于单元测试直接调用。

use std::collections::BTreeMap;
use std::sync::Mutex;

use secretbox_core::{
    backup_filename, build_file, generate_recovery_key, parse_file, Db, Item, SecretboxError,
    Snapshot, Version,
};
use tauri::{Manager, State};

/// 应用运行时状态：数据库句柄 + 解锁后的派生密钥（仅存内存）。
/// 清除痕迹后 db 为 None（文件已删除），首次设置主密码时按 db_path 重建。
pub struct AppState {
    db_path: String,
    db: Mutex<Option<Db>>,
    key: Mutex<Option<Vec<u8>>>,
}

/// 启动 Tauri 应用。数据库打开失败时返回错误，由入口决定如何报告。
pub fn run(db_path: &str) -> Result<(), String> {
    let state = open_state(db_path)?;
    tauri::Builder::default()
        // 单实例：二次启动不开新窗口，聚焦已运行的主窗口后自动退出。
        // 官方要求此插件最先注册（先于其他插件与窗口创建）。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            apply_window_theme,
            get_status,
            unlock,
            lock,
            setup_password,
            upgrade_v1,
            verify_password,
            change_password,
            recover_password,
            get_recovery_key,
            regenerate_recovery_key,
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
            update_settings,
            export_snapshot,
            import_snapshot,
            wipe,
            save_snapshot_file
        ])
        .run(tauri::generate_context!())
        .map_err(|err| format!("Tauri 应用运行异常: {err}"))?;
    Ok(())
}

fn open_state(db_path: &str) -> Result<AppState, String> {
    let db = Db::open(db_path).map_err(|err| format!("数据库初始化失败: {err}"))?;
    Ok(AppState {
        db_path: db_path.to_string(),
        db: Mutex::new(Some(db)),
        key: Mutex::new(None),
    })
}

// ---------- 原生窗口配色（Windows：边框/标题栏跟随应用主题） ----------

/// 把窗口边框与标题栏颜色设为同一个应用主题色，避免系统强调色（如粉橙色边框）
/// 与深色 UI 冲突。颜色独立于 src/style.css 变量，改动需同步修改下方硬编码值，
/// COLORREF 字节序为 0x00BBGGRR。配色失败只静默忽略（纯装饰，不值得中断）。
fn apply_window_theme_impl(window: &tauri::WebviewWindow, theme: &str) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::COLORREF;
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR,
        };

        // 边框与标题栏同色（不做层次区分）：暗色 #202329，亮色 #ffffff
        let (border, caption) = if theme == "light" {
            (0x00FFFFFF, 0x00FFFFFF) // #ffffff
        } else {
            (0x00292320, 0x00292320) // #202329
        };

        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_BORDER_COLOR,
                    &COLORREF(border) as *const COLORREF as *const std::ffi::c_void,
                    std::mem::size_of::<COLORREF>() as u32,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_CAPTION_COLOR,
                    &COLORREF(caption) as *const COLORREF as *const std::ffi::c_void,
                    std::mem::size_of::<COLORREF>() as u32,
                );
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, theme);
    }
}

#[tauri::command]
fn apply_window_theme(window: tauri::WebviewWindow, theme: String) {
    apply_window_theme_impl(&window, &theme);
}

// ---------- 命令实现（独立于 Tauri 宏，可测试） ----------

/// 状态：是否已设置主密码、当前是否已解锁、是否为待升级的 v1 旧库。
/// 清除痕迹后数据库文件已删除，视为未设置主密码。
fn status_impl(state: &AppState) -> Result<serde_json::Value, String> {
    if state.db.lock().expect("数据库锁不可中毒").is_none() {
        return Ok(serde_json::json!({
            "has_password": false,
            "unlocked": false,
            "legacy": false,
        }));
    }
    with_db(state, |db| {
        Ok(serde_json::json!({
            "has_password": db.has_master_password(),
            "unlocked": current_key(state).is_some(),
            "legacy": db.has_master_password() && !db.is_v2(),
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

/// 首次设置主密码。清除痕迹后数据库未打开，这里按路径重建。
/// 同时生成恢复密钥（强制流程，前端确认页保证用户抄写，见 ADR-0003），
/// 返回明文恢复密钥供前端展示。
fn setup_password_impl(state: &AppState, password: &str) -> Result<serde_json::Value, String> {
    let trimmed = password.trim();
    if trimmed.chars().count() < 4 {
        return Err("主密码至少 4 个字符".to_string());
    }
    {
        let mut guard = state.db.lock().expect("数据库锁不可中毒");
        if guard.is_none() {
            let db = Db::open(&state.db_path).map_err(|err| err.to_string())?;
            *guard = Some(db);
        }
    }
    let has_password = with_db(state, |db| Ok(db.has_master_password()))?;
    if has_password {
        return Err("主密码已设置".to_string());
    }
    let key = with_db(state, |db| {
        db.setup_master_password(trimmed)
            .map_err(|err| err.to_string())
    })?;
    let recovery_key = with_db(state, |db| {
        let code = generate_recovery_key().map_err(|err| err.to_string())?;
        db.set_recovery_key(&code, &key).map_err(|err| err.to_string())?;
        Ok(code)
    })?;
    *state.key.lock().expect("密钥锁不可中毒") = Some(key);
    Ok(serde_json::json!({ "ok": true, "recovery_key": recovery_key }))
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

/// 修改主密码：先验证旧密码，再重新包装 DEK（条目密文不动，见 ADR-0003）。
fn change_password_impl(
    state: &AppState,
    old_password: &str,
    new_password: &str,
) -> Result<serde_json::Value, String> {
    require_unlocked(state)?;
    if old_password.is_empty() {
        return Err("密码不能为空".to_string());
    }
    if new_password.chars().count() < 4 {
        return Err("新密码至少 4 位".to_string());
    }
    let new_key = with_db(state, |db| {
        // 旧密码错误会在这里报"解密失败"，改密不会发生
        let old_key = db.unlock(old_password).map_err(|err| err.to_string())?;
        db.change_password(&old_key, new_password)
            .map_err(|err| err.to_string())
    })?;
    let key_len = new_key.len();
    *state.key.lock().expect("密钥锁不可中毒") = Some(new_key);
    Ok(serde_json::json!({ "key_len": key_len }))
}

/// 忘记主密码的救援：用恢复密钥解开 DEK，重设主密码（只重包装，数据不变）。
/// 与 change_password 的区别：在锁定状态下调用（这正是它的用途），
/// 凭恢复密钥而非旧主密码验证，成功后直接置为解锁态。见 ADR-0003。
fn recover_password_impl(
    state: &AppState,
    recovery_key: &str,
    new_password: &str,
) -> Result<serde_json::Value, String> {
    if recovery_key.trim().is_empty() {
        return Err("请输入恢复密钥".to_string());
    }
    if new_password.chars().count() < 4 {
        return Err("新密码至少 4 位".to_string());
    }
    let dek = with_db(state, |db| {
        db.unlock_with_recovery_key(recovery_key)
            .map_err(|err| err.to_string())
    })?;
    // 错误恢复密钥到不了这里；改密只重包装 DEK，恢复密钥继续有效
    with_db(state, |db| {
        db.change_password(&dek, new_password)
            .map_err(|err| err.to_string())
    })?;
    *state.key.lock().expect("密钥锁不可中毒") = Some(dek);
    Ok(serde_json::json!({ "ok": true }))
}

/// v1 旧库的待迁移条目明文（含历史版本），升级时在新库中重加密。
struct HarvestedItem {
    title: String,
    category: String,
    note: String,
    value: String,
    created_at: String,
    updated_at: String,
    versions: Vec<HarvestedVersion>,
}

struct HarvestedVersion {
    version: i64,
    snapshot: String,
    created_at: String,
}

/// v1 旧库强制升级向导（见 ADR-0003）：验证主密码 → 收割全部明文 →
/// 归档旧文件 → 建新 v2 库并重加密导入 → 强制生成恢复密钥。
/// 成功后直接进入解锁态；任何失败都会回滚（旧库原样恢复，可重试）。
fn upgrade_v1_impl(state: &AppState, password: &str) -> Result<serde_json::Value, String> {
    // 1. 校验并收割旧库明文
    let harvested = with_db(state, |db| {
        if !db.has_master_password() {
            return Err("未设置主密码".to_string());
        }
        if db.is_v2() {
            return Err("数据库已是 v2 格式，无需升级".to_string());
        }
        let key = db.unlock(password).map_err(|_| "主密码不正确".to_string())?;
        let mut items = Vec::new();
        for it in db.list_items().map_err(|err| err.to_string())? {
            let full = db.get_item(&key, it.id).map_err(|err| err.to_string())?;
            let mut versions = Vec::new();
            for v in db.list_versions(it.id).map_err(|err| err.to_string())? {
                let snapshot = db
                    .get_version_snapshot(&key, it.id, v.version)
                    .map_err(|err| err.to_string())?;
                versions.push(HarvestedVersion {
                    version: v.version,
                    snapshot,
                    created_at: v.created_at,
                });
            }
            items.push(HarvestedItem {
                title: full.title,
                category: full.category,
                note: full.note,
                value: full.value,
                created_at: full.created_at,
                updated_at: full.updated_at,
                versions,
            });
        }
        Ok(items)
    })?;

    // 2. 关闭旧库并归档（重命名留作后路；Windows 杀毒可能瞬时锁文件，重试）
    let old_db = state.db.lock().expect("数据库锁不可中毒").take();
    if let Some(db) = old_db {
        db.checkpoint_wal().map_err(|err| err.to_string())?;
        drop(db);
    }
    let db_path = state.db_path.clone();
    let bak_path = format!("{}.v1.bak", db_path);
    archive_files(&db_path, &bak_path).map_err(|err| {
        // 归档失败意味着旧库原样未动，重新打开即可
        reopen_old(state, &db_path, &bak_path, false);
        format!("归档旧数据库失败: {err}")
    })?;

    // 3. 建新 v2 库并导入（失败则回滚）
    let build = (|| -> Result<(Vec<u8>, String), String> {
        let mut db = Db::open(&db_path).map_err(|err| err.to_string())?;
        let dek = db.setup_master_password(password).map_err(|err| err.to_string())?;
        let mut snap = db.get_snapshot().map_err(|err| err.to_string())?;
        snap.items = Vec::new();
        for h in &harvested {
            let value = secretbox_core::encrypt(&dek, &h.value).map_err(|err| err.to_string())?;
            let mut versions = Vec::new();
            for v in &h.versions {
                let encrypted = secretbox_core::encrypt(&dek, &v.snapshot)
                    .map_err(|err| err.to_string())?;
                versions.push(secretbox_core::SnapshotVersion {
                    version: v.version,
                    snapshot: encrypted,
                    created: v.created_at.clone(),
                });
            }
            snap.items.push(secretbox_core::SnapshotItem {
                title: h.title.clone(),
                category: h.category.clone(),
                note: h.note.clone(),
                value,
                created: h.created_at.clone(),
                updated: h.updated_at.clone(),
                versions,
            });
        }
        db.restore_from_snapshot(&snap).map_err(|err| err.to_string())?;
        let code = generate_recovery_key().map_err(|err| err.to_string())?;
        db.set_recovery_key(&code, &dek).map_err(|err| err.to_string())?;
        db.checkpoint_wal().map_err(|err| err.to_string())?;
        Ok((dek, code))
    })();

    match build {
        Ok((dek, code)) => {
            let db = Db::open(&db_path).map_err(|err| err.to_string())?;
            *state.db.lock().expect("数据库锁不可中毒") = Some(db);
            *state.key.lock().expect("密钥锁不可中毒") = Some(dek);
            Ok(serde_json::json!({ "ok": true, "recovery_key": code }))
        }
        Err(err) => {
            reopen_old(state, &db_path, &bak_path, true);
            Err(err)
        }
    }
}

/// 把数据库文件（含 -wal/-shm）改名为归档名。带重试。
fn archive_files(db_path: &str, bak_path: &str) -> std::io::Result<()> {
    let mut last_err = None;
    for _ in 0..5 {
        // 上一次升级的遗留归档直接覆盖（当前库才是数据源头）
        let _ = std::fs::remove_file(bak_path);
        match std::fs::rename(db_path, bak_path) {
            Ok(()) => {
                for suffix in ["-wal", "-shm"] {
                    let _ = std::fs::remove_file(format!("{db_path}{suffix}"));
                }
                return Ok(());
            }
            Err(err) => last_err = Some(err),
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("重命名失败")))
}

/// 升级失败的回滚：删除新建的半成品文件，把归档改回原名并重新打开进状态。
/// `archived` 为 false 表示归档本身失败、旧库未动过，只需重新打开。
fn reopen_old(state: &AppState, db_path: &str, bak_path: &str, archived: bool) {
    if archived {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{db_path}{suffix}"));
        }
        for _ in 0..5 {
            if std::fs::rename(bak_path, db_path).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    if let Ok(db) = Db::open(db_path) {
        *state.db.lock().expect("数据库锁不可中毒") = Some(db);
    }
}

/// 查看当前恢复密钥（仅解锁后；会话密钥即 DEK，锁定态不可得，见 ADR-0003）。
fn get_recovery_key_impl(state: &AppState) -> Result<serde_json::Value, String> {
    let key = require_unlocked(state)?;
    with_db(state, |db| {
        let code = db.get_recovery_key(&key).map_err(|err| err.to_string())?;
        Ok(serde_json::json!({ "recovery_key": code }))
    })
}

/// 重新生成恢复密钥：验证主密码后生成新码并重新包装 DEK，旧码立即作废。
/// 返回新恢复密钥（规范分组码），调用方必须展示给用户保存。
fn regenerate_recovery_key_impl(
    state: &AppState,
    master_password: &str,
) -> Result<serde_json::Value, String> {
    let key = require_unlocked(state)?;
    with_db(state, |db| {
        // 主密码错误会在这里报"解密失败"，重生成不会发生
        db.unlock(master_password).map_err(|err| err.to_string())?;
        let code = generate_recovery_key().map_err(|err| err.to_string())?;
        db.set_recovery_key(&code, &key).map_err(|err| err.to_string())?;
        Ok(serde_json::json!({ "recovery_key": code }))
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

/// 导出快照迁移文件：返回建议文件名与文件内容（base64 文本）。
/// 加密使用独立的"迁移口令"，与主密码无关。未解锁时拒绝。
fn export_snapshot_impl(
    state: &AppState,
    password: &str,
) -> Result<serde_json::Value, String> {
    require_unlocked(state)?;
    if password.trim().chars().count() < 4 {
        return Err("迁移口令至少 4 个字符".to_string());
    }
    let content = with_db(state, |db| {
        let snap = db.get_snapshot().map_err(|err| err.to_string())?;
        build_file(&snap, password).map_err(|err| err.to_string())
    })?;
    Ok(serde_json::json!({
        "filename": backup_filename(),
        "content": content,
    }))
}

/// 导入快照迁移文件并覆盖本地数据。成功后弃用当前会话，需重新解锁。
fn import_snapshot_impl(
    state: &AppState,
    password: &str,
    content: &str,
) -> Result<serde_json::Value, String> {
    require_unlocked(state)?;
    let snap: Snapshot = parse_file(content, password).map_err(|err| match err {
        SecretboxError::MigrationFormatInvalid => "迁移文件格式无效".to_string(),
        SecretboxError::MigrationPassphraseWrong => "迁移口令错误或文件已损坏".to_string(),
        SecretboxError::MigrationContentInvalid => "迁移文件内容无效".to_string(),
        other => other.to_string(),
    })?;
    let item_count = snap.items.len();
    let has_password = snap.has_password;
    with_db(state, |db| {
        db.restore_from_snapshot(&snap)
            .map_err(|err| format!("恢复数据失败: {err}"))
    })?;
    // 弃用旧会话（与 Go 版一致：导入后需用原主密码重新解锁）
    *state.key.lock().expect("密钥锁不可中毒") = None;
    Ok(serde_json::json!({
        "imported": true,
        "has_password": has_password,
        "items": item_count,
    }))
}

/// 清除本地全部数据：清空表后关闭连接并删除数据库文件（含 WAL/SHM）。
/// 导出前置与二次确认由前端把关。未解锁时拒绝。
fn wipe_impl(state: &AppState) -> Result<serde_json::Value, String> {
    require_unlocked(state)?;
    let db = state
        .db
        .lock()
        .expect("数据库锁不可中毒")
        .take()
        .ok_or_else(|| "数据库未打开".to_string())?;
    db.wipe_and_remove_files().map_err(|err| err.to_string())?;
    *state.key.lock().expect("密钥锁不可中毒") = None;
    Ok(serde_json::json!({ "wiped": true }))
}

/// 把导出的快照内容保存为文件：弹原生"另存为"对话框，返回保存路径；取消返回空串。
fn save_snapshot_file_impl(
    filename: &str,
    content: &str,
) -> Result<String, String> {
    let path = rfd::FileDialog::new().set_file_name(filename).save_file();
    match path {
        Some(path) => {
            std::fs::write(&path, content)
                .map_err(|err| format!("写入文件失败: {err}"))?;
            Ok(path.display().to_string())
        }
        None => Ok(String::new()),
    }
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
fn change_password(
    state: State<AppState>,
    old_password: String,
    new_password: String,
) -> Result<serde_json::Value, String> {
    change_password_impl(&state, &old_password, &new_password)
}

#[tauri::command]
fn recover_password(
    state: State<AppState>,
    recovery_key: String,
    new_password: String,
) -> Result<serde_json::Value, String> {
    recover_password_impl(&state, &recovery_key, &new_password)
}

#[tauri::command]
fn upgrade_v1(state: State<AppState>, password: String) -> Result<serde_json::Value, String> {
    upgrade_v1_impl(&state, &password)
}

#[tauri::command]
fn get_recovery_key(state: State<AppState>) -> Result<serde_json::Value, String> {
    get_recovery_key_impl(&state)
}

#[tauri::command]
fn regenerate_recovery_key(
    state: State<AppState>,
    master_password: String,
) -> Result<serde_json::Value, String> {
    regenerate_recovery_key_impl(&state, &master_password)
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

#[tauri::command]
fn export_snapshot(state: State<AppState>, password: String) -> Result<serde_json::Value, String> {
    export_snapshot_impl(&state, &password)
}

#[tauri::command]
fn import_snapshot(
    state: State<AppState>,
    password: String,
    content: String,
) -> Result<serde_json::Value, String> {
    import_snapshot_impl(&state, &password, &content)
}

#[tauri::command]
fn wipe(state: State<AppState>) -> Result<serde_json::Value, String> {
    wipe_impl(&state)
}

#[tauri::command]
fn save_snapshot_file(filename: String, content: String) -> Result<String, String> {
    save_snapshot_file_impl(&filename, &content)
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

    /// 打开一个全新空库的测试状态（v2 格式，尚未设置主密码）。
    fn open_fresh_state() -> (AppState, std::path::PathBuf) {
        let tmp = std::env::temp_dir().join(format!(
            "secretbox-lib-fresh-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let db_path = tmp.join("fresh.db");
        let state = open_state(db_path.to_str().unwrap()).unwrap();
        (state, tmp)
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

    #[test]
    fn 首次设置同时生成恢复密钥_凭恢复密钥可解锁() {
        let (state, _tmp) = open_fresh_state();
        let result = setup_password_impl(&state, "first-pass-123").unwrap();
        let recovery_key = result["recovery_key"].as_str().expect("返回恢复密钥");
        assert!(result["ok"].as_bool().unwrap());

        // 后端已写入恢复侧包装；会话密钥（DEK）可被恢复密钥解开
        // （小写输入也必须通过——归一化在核心层完成）
        let dek = current_key(&state).expect("设置后已解锁");
        with_db(&state, |db| {
            assert!(db.has_recovery_key(), "设置后必须已存在恢复密钥包装");
            let unwrapped = db
                .unlock_with_recovery_key(&recovery_key.to_lowercase())
                .map_err(|err| err.to_string())?;
            assert_eq!(unwrapped, dek, "恢复密钥必须解开同一个 DEK");
            // 错误恢复密钥被拒
            assert!(db.unlock_with_recovery_key("AAAA-BBBB-CCCC-DDDD").is_err());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn 救援流程_恢复密钥重设主密码_旧密码失效数据不变() {
        let (state, _tmp) = open_fresh_state();
        let result = setup_password_impl(&state, "lost-pass-123").unwrap();
        let recovery_key = result["recovery_key"].as_str().unwrap();
        create_item_impl(&state, "条目一", "分类", "", "救援前内容").unwrap();
        // 锁定：模拟忘记主密码的状态
        lock_impl(&state).unwrap();

        // 锁定状态下救援页可读状态，但读条目被拒绝（无旁路）
        assert_eq!(list_items_impl(&state).unwrap_err(), "未解锁");

        // 输入校验
        assert_eq!(
            recover_password_impl(&state, "", "new-pass-123").unwrap_err(),
            "请输入恢复密钥"
        );
        assert_eq!(
            recover_password_impl(&state, recovery_key, "abc").unwrap_err(),
            "新密码至少 4 位"
        );
        // 错误恢复密钥被拒（后端语义"恢复密钥不正确"）
        assert!(recover_password_impl(&state, "AAAA-BBBB-CCCC-DDDD", "new-pass-123").is_err());
        // 仍然锁定
        assert_eq!(list_items_impl(&state).unwrap_err(), "未解锁");

        // 正确救援：锁定状态下直接进入解锁态，数据完整
        recover_password_impl(&state, recovery_key, "new-pass-123").unwrap();
        let items = list_items_impl(&state).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(get_item_impl(&state, items[0].id).unwrap().value, "救援前内容");

        // 旧主密码失效，新主密码可解锁；恢复密钥继续有效（无需重新生成）
        lock_impl(&state).unwrap();
        assert!(unlock_impl(&state, "lost-pass-123").is_err());
        unlock_impl(&state, "new-pass-123").unwrap();
        lock_impl(&state).unwrap();
        let result2 = setup_probe_recovery(&state, recovery_key);
        assert!(result2, "救援后恢复密钥必须仍然有效");
    }

    /// 验证给定恢复密钥能否解开 DEK（不改变会话状态）。
    fn setup_probe_recovery(state: &AppState, recovery_key: &str) -> bool {
        with_db(state, |db| {
            db.unlock_with_recovery_key(recovery_key)
                .map(|_| ())
                .map_err(|err| err.to_string())
        })
        .is_ok()
    }

    #[test]
    fn 恢复密钥生命周期_查看与重生成() {
        let (state, _tmp) = open_fresh_state();
        let result = setup_password_impl(&state, "first-pass-123").unwrap();
        let original = result["recovery_key"].as_str().unwrap();

        // 锁定态不可查看（设置入口只在解锁后的主界面，机制上双保险）
        lock_impl(&state).unwrap();
        assert_eq!(get_recovery_key_impl(&state).unwrap_err(), "未解锁");

        // 解锁后查看：与生成时一致
        unlock_impl(&state, "first-pass-123").unwrap();
        let viewed = get_recovery_key_impl(&state).unwrap()["recovery_key"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(viewed, original, "查看结果必须与生成时一致");

        // 重生成：主密码错误被拒
        assert!(regenerate_recovery_key_impl(&state, "错误密码").is_err());
        // 正确：新码 != 旧码
        let regenerated = regenerate_recovery_key_impl(&state, "first-pass-123").unwrap()
            ["recovery_key"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(regenerated, original, "重生成必须产生新码");

        // 旧码作废：无法解锁 DEK；查看显示新码
        let dek = current_key(&state).unwrap();
        with_db(&state, |db| {
            assert!(db.unlock_with_recovery_key(original).is_err(), "旧恢复密钥必须作废");
            db.unlock_with_recovery_key(&regenerated).map_err(|err| err.to_string())?;
            let viewed_now = db.get_recovery_key(&dek).map_err(|err| err.to_string())?;
            assert_eq!(viewed_now.as_deref(), Some(regenerated.as_str()));
            Ok(())
        })
        .unwrap();

        // 新码可走通救援流程（与 03 衔接）
        lock_impl(&state).unwrap();
        recover_password_impl(&state, &regenerated, "rescued-pass-1").unwrap();
        assert!(list_items_impl(&state).is_ok());
    }

    #[test]
    fn 升级向导_v1库升级为v2_数据完整且不可绕过() {
        let (state, tmp) = open_test_state();
        let db_path = tmp.join("golden.db");
        let bak_path = std::path::Path::new(&format!("{}.v1.bak", db_path.display())).to_path_buf();

        // 升级前状态：legacy 标记，数据可读（收割 3 条明文作为期望）
        let status = status_impl(&state).unwrap();
        assert_eq!(status["legacy"], true, "v1 库必须报告 legacy");
        unlock_impl(&state, "golden-test-password").unwrap();
        let before = list_items_impl(&state).unwrap();
        let mut expected = Vec::new();
        for it in &before {
            let full = get_item_impl(&state, it.id).unwrap();
            let mut versions = Vec::new();
            for v in list_versions_impl(&state, it.id).unwrap() {
                versions.push(get_version_snapshot_impl(&state, it.id, v.version).unwrap());
            }
            expected.push((full.value, versions.len()));
        }
        lock_impl(&state).unwrap();

        // 主密码错误：升级失败，旧库原样保留可重试
        assert!(upgrade_v1_impl(&state, "错误的密码").is_err());
        let status = status_impl(&state).unwrap();
        assert_eq!(status["legacy"], true, "失败后旧库必须原样保留");
        unlock_impl(&state, "golden-test-password").unwrap();
        lock_impl(&state).unwrap();

        // 正确升级：返回恢复密钥，进入解锁态
        let result = upgrade_v1_impl(&state, "golden-test-password").unwrap();
        let recovery_key = result["recovery_key"].as_str().unwrap();
        assert_eq!(result["ok"], true);

        // 新库是 v2，legacy 消失，旧文件已归档
        let status = status_impl(&state).unwrap();
        assert_eq!(status["legacy"], false);
        assert!(!db_path.join(".v1.bak").exists());
        assert!(bak_path.exists(), "旧库必须归档留作后路");
        with_db(&state, |db| {
            Ok(db.is_v2() && db.has_recovery_key())
        })
        .unwrap();

        // 数据完整：3 条 + 历史版本数量一致，明文逐条一致
        let items = list_items_impl(&state).unwrap();
        assert_eq!(items.len(), before.len());
        for (it, (want_value, want_versions)) in items.iter().zip(expected.iter()) {
            let full = get_item_impl(&state, it.id).unwrap();
            assert_eq!(full.value, *want_value, "条目 {} 明文", it.id);
            let versions = list_versions_impl(&state, it.id).unwrap();
            assert_eq!(versions.len(), *want_versions, "条目 {} 版本数", it.id);
        }

        // 救援链可用：锁定 → 恢复密钥重设主密码
        lock_impl(&state).unwrap();
        recover_password_impl(&state, recovery_key, "rescued-pass-1").unwrap();
        assert!(list_items_impl(&state).is_ok());
    }

    #[test]
    fn v1旧库改密被拒绝_只读导入源() {
        let (state, _tmp) = open_test_state();
        unlock_impl(&state, "golden-test-password").unwrap();
        let err = change_password_impl(&state, "golden-test-password", "new-pass-123").unwrap_err();
        assert!(err.contains("只读"), "v1 旧库改密必须被拒绝: {err}");
    }

    #[test]
    fn 改密后旧密码失效_新密码可解锁且数据完整() {
        // v2 全新库：设置主密码并写入两条数据
        let (state, _tmp) = open_fresh_state();
        setup_password_impl(&state, "old-pass-123").unwrap();
        create_item_impl(&state, "条目一", "分类", "", "内容一").unwrap();
        create_item_impl(&state, "条目二", "", "备注", "内容二").unwrap();
        let before = list_items_impl(&state).unwrap();
        let mut values = Vec::new();
        for it in &before {
            values.push(get_item_impl(&state, it.id).unwrap().value);
        }

        // 校验不过关的输入直接拒绝
        assert_eq!(
            change_password_impl(&state, "", "new-pass").unwrap_err(),
            "密码不能为空"
        );
        assert_eq!(
            change_password_impl(&state, "old-pass-123", "abc").unwrap_err(),
            "新密码至少 4 位"
        );

        // 旧密码错误：改密失败，会话密钥仍可读数据
        assert!(change_password_impl(&state, "错误旧密码", "new-pass-123").is_err());
        assert!(get_item_impl(&state, before[0].id).is_ok());

        // 正确改密：成功，条目密文未动、明文不变
        change_password_impl(&state, "old-pass-123", "new-pass-123").unwrap();
        let after = list_items_impl(&state).unwrap();
        assert_eq!(after.len(), before.len());
        for (it, value) in after.iter().zip(values.iter()) {
            assert_eq!(get_item_impl(&state, it.id).unwrap().value, *value);
        }

        // 锁定后只有新密码能解锁
        lock_impl(&state).unwrap();
        assert!(unlock_impl(&state, "old-pass-123").is_err());
        unlock_impl(&state, "new-pass-123").unwrap();
    }

    #[test]
    fn 导出导入快照回环_导入后需重新解锁() {
        let (state, _tmp) = open_test_state();
        unlock_impl(&state, "golden-test-password").unwrap();

        // 口令过短拒绝
        assert_eq!(
            export_snapshot_impl(&state, "abc").unwrap_err(),
            "迁移口令至少 4 个字符"
        );

        // 导出
        let exported = export_snapshot_impl(&state, "迁移口令123").unwrap();
        let filename = exported["filename"].as_str().unwrap();
        let content = exported["content"].as_str().unwrap();
        assert!(filename.starts_with("secretbox-backup-") && filename.ends_with(".secretbox"));

        // 导入（口令错误被拒）
        assert_eq!(
            import_snapshot_impl(&state, "错误口令", content).unwrap_err(),
            "迁移口令错误或文件已损坏"
        );
        let result = import_snapshot_impl(&state, "迁移口令123", content).unwrap();
        assert_eq!(result["imported"], true);
        assert_eq!(result["items"], 3);
        assert_eq!(result["has_password"], true);

        // 导入后旧会话被弃用
        assert!(list_items_impl(&state).unwrap_err() == "未解锁");
        // 原主密码可重新解锁，数据完整
        unlock_impl(&state, "golden-test-password").unwrap();
        assert_eq!(list_items_impl(&state).unwrap().len(), 3);
    }

    #[test]
    fn 清除痕迹后数据库文件被删除_可重新设置主密码() {
        let (state, tmp) = open_test_state();
        let db_copy = tmp.join("golden.db");
        unlock_impl(&state, "golden-test-password").unwrap();

        // 未解锁时拒绝
        lock_impl(&state).unwrap();
        assert_eq!(wipe_impl(&state).unwrap_err(), "未解锁");

        unlock_impl(&state, "golden-test-password").unwrap();
        let wiped = wipe_impl(&state).unwrap();
        assert_eq!(wiped["wiped"], true);
        assert!(!db_copy.exists(), "数据库文件必须被删除");
        assert!(!tmp.join("golden.db-wal").exists(), "WAL 必须被删除");
        assert!(!tmp.join("golden.db-shm").exists(), "SHM 必须被删除");

        // 清除后状态为未设置主密码，可重新设置
        let status = status_impl(&state).unwrap();
        assert_eq!(status["has_password"], false);
        assert_eq!(status["unlocked"], false);
        setup_password_impl(&state, "brand-new-pass").unwrap();
        assert_eq!(list_items_impl(&state).unwrap().len(), 0);
    }
}
