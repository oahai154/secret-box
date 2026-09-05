// main.rs - 桌面应用入口：解析 --db 参数并启动 Tauri。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

fn main() {
    let db_path = parse_db_arg(std::env::args().skip(1)).unwrap_or_else(default_db_path);

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
fn parse_db_arg(args: impl Iterator<Item = String>) -> Option<PathBuf> {
    let mut iter = args.peekable();
    while let Some(arg) = iter.next() {
        if arg == "--db" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--db=") {
            return Some(PathBuf::from(value));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn db_参数_空格分隔() {
        assert_eq!(
            parse_db_arg(args(&["--db", "D:\\portable\\sb.db"]).into_iter()),
            Some(PathBuf::from("D:\\portable\\sb.db"))
        );
    }

    #[test]
    fn db_参数_等号分隔() {
        assert_eq!(
            parse_db_arg(args(&["--db=C:\\data\\sb.db"]).into_iter()),
            Some(PathBuf::from("C:\\data\\sb.db"))
        );
    }

    #[test]
    fn db_参数_忽略其他参数() {
        assert_eq!(
            parse_db_arg(args(&["--port", "8080", "--db", "x.db", "--no-open"]).into_iter()),
            Some(PathBuf::from("x.db"))
        );
    }

    #[test]
    fn db_参数_缺失时返回_none() {
        assert_eq!(parse_db_arg(args(&[]).into_iter()), None);
        assert_eq!(parse_db_arg(args(&["--db"]).into_iter()), None);
        assert_eq!(parse_db_arg(args(&["--port", "8080"]).into_iter()), None);
    }

    #[test]
    fn 默认路径_优先_localappdata() {
        // 以真实环境变量解析，仅断言目录名与文件名，不依赖具体盘符。
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            if !dir.is_empty() {
                assert_eq!(
                    default_db_path(),
                    PathBuf::from(dir).join("SecretBox").join("secretbox.db")
                );
                return;
            }
        }
        assert!(default_db_path().ends_with("secretbox.db"));
    }
}
