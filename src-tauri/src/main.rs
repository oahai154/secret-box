// main.rs - 桌面应用入口：解析 --db 参数并启动 Tauri。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

fn main() {
    let db_path = parse_db_arg().unwrap_or_else(default_db_path);

    // 确保数据目录存在（与 Go 版一致）
    if let Some(parent) = db_path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!("创建数据目录失败: {err}");
            std::process::exit(1);
        }
    }

    if let Err(err) = secretbox_lib::run(db_path.to_str().unwrap_or_default()) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

/// 解析 `--db <路径>` 或 `--db=<路径>`。
fn parse_db_arg() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--db" {
            return args.get(i + 1).map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--db=") {
            return Some(PathBuf::from(value));
        }
        i += 1;
    }
    None
}

/// 默认数据库路径，与 Go 版 defaultDBPath 一致：
/// Windows 用 %LOCALAPPDATA%，其次 XDG_DATA_HOME / APPDATA，
/// 其余平台落在用户主目录（macOS: Library/Application Support；其他: .local/share）。
fn default_db_path() -> PathBuf {
    for key in ["LOCALAPPDATA", "XDG_DATA_HOME", "APPDATA"] {
        if let Ok(dir) = std::env::var(key) {
            if !dir.is_empty() {
                return PathBuf::from(dir).join("SecretBox").join("secretbox.db");
            }
        }
    }
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        if !home.is_empty() {
            if cfg!(target_os = "macos") {
                return PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("SecretBox")
                    .join("secretbox.db");
            }
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("SecretBox")
                .join("secretbox.db");
        }
    }
    PathBuf::from("secretbox.db")
}
