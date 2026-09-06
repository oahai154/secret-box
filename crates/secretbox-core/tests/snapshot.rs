//! 工单 #06 验收：v1 快照导入 + 清除痕迹。
//!
//! v2 起快照导出/导入携带双份 DEK 包装（ADR-0003），v2 语义的快照回环测试
//! 在 dek.rs（含主密码/恢复密钥双凭据导入）；本文件保留 v1 快照的只读导入
//! 路径回归（fixture 为 Go 版生成的旧格式快照能力等价物）。

use secretbox_core::{build_file, parse_file, Db, Snapshot};
use serde_json::json;

const PASSPHRASE: &str = "migration-pass-#06";

/// 导出→导入回环：快照还原后全部条目/版本/盐值与导出前一致。
#[test]
fn 快照导出导入回环_数据与盐值一致() {
    let (db, key, _tmp) = setup();
    let before = dump_plaintext(&db, &key);
    let salt_before = db.salt_b64().unwrap();

    let snap = db.get_snapshot().expect("读取快照");
    let content = build_file(&snap, PASSPHRASE).expect("构建迁移文件");
    let parsed: Snapshot = parse_file(&content, PASSPHRASE).expect("解析迁移文件");
    assert_eq!(parsed.salt_b64, salt_before);
    assert!(parsed.has_password);

    // 还原到同一个库（覆盖式）
    let mut db2 = open_fresh();
    db2.restore_from_snapshot(&parsed).expect("还原快照");
    let key2 = db2.unlock("golden-test-password").expect("原主密码可解锁");
    let after = dump_plaintext(&db2, &key2);
    assert_eq!(after, before, "快照还原后数据必须一致");
}

/// 清除痕迹：清空数据 + 数据库文件（含 WAL/SHM）全部不存在。
#[test]
fn 清除痕迹后数据库文件全部不存在() {
    let (db, key, db_path) = setup();
    // 先写点数据，确保 WAL 里有内容
    db.update_item(&key, 1, "标题", "分类", "备注", "改一下").expect("更新条目");

    let wal = format!("{}-wal", db_path.display());
    let shm = format!("{}-shm", db_path.display());
    db.checkpoint_wal().ok(); // 触发 wal/shm 存在
    db.wipe_and_remove_files().expect("清除痕迹");

    assert!(!db_path.exists(), "主数据库文件必须被删除");
    assert!(!std::path::Path::new(&wal).exists(), "WAL 文件必须被删除");
    assert!(!std::path::Path::new(&shm).exists(), "SHM 文件必须被删除");
}

/// 复制 fixture 并解锁，返回 (数据库, 密钥, 数据库路径)。
fn setup() -> (Db, Vec<u8>, std::path::PathBuf) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden.db");
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-snapshot-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    let db_path = tmp.join("snapshot.db");
    let mut copied = false;
    for _ in 0..5 {
        match std::fs::copy(fixture, &db_path) {
            Ok(_) => {
                copied = true;
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(200)),
        }
    }
    assert!(copied, "复制 fixture 失败");
    let db = Db::open(db_path.to_str().unwrap()).expect("打开副本");
    let key = db.unlock("golden-test-password").expect("解锁 fixture");
    (db, key, db_path)
}

/// 打开一个全新空数据库。
fn open_fresh() -> Db {
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-snapshot-fresh-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    Db::open(tmp.join("fresh.db").to_str().unwrap()).expect("打开新库")
}

/// 导出全部条目（含历史版本快照明文）。
fn dump_plaintext(db: &Db, key: &[u8]) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    for it in db.list_items().expect("列出条目") {
        let full = db.get_item(key, it.id).expect("读取条目");
        let mut versions = Vec::new();
        for v in db.list_versions(it.id).expect("列出版本") {
            let snapshot = db
                .get_version_snapshot(key, it.id, v.version)
                .expect("读取快照");
            versions.push(json!({
                "version": v.version,
                "created_at": v.created_at,
                "snapshot": snapshot,
            }));
        }
        items.push(json!({
            "id": full.id,
            "title": full.title,
            "category": full.category,
            "note": full.note,
            "created_at": full.created_at,
            "updated_at": full.updated_at,
            "value": full.value,
            "version_count": versions.len() as i64,
            "versions": versions,
        }));
    }
    items
}
