//! 工单 #06 验收：快照导出/导入 + 清除痕迹，跨语言双向互通。
//!
//! 用法（配合 Go 测试工具 golden_fixture_test.go 中的 TestSnapshot*Tool）：
//!
//! 方向 A（Rust 导出 → Go 导入比对）：
//!   SECRETBOX_SNAPSHOT_OUT=<目录> cargo test -p secretbox-core --test snapshot -- 快照导出
//!   SECRETBOX_SNAPSHOT_IMPORT_FILE=<目录>/rust-export.secretbox \
//!   SECRETBOX_SNAPSHOT_IMPORT_PASSWORD=migration-pass-#06 \
//!   SECRETBOX_SNAPSHOT_IMPORT_DB=<目录>/go-imported.db \
//!   SECRETBOX_SNAPSHOT_IMPORT_OUT=<目录>/go-import-dump.json \
//!   SECRETBOX_SNAPSHOT_IMPORT_EXPECT=<目录>/rust-export-expected.json \
//!   SECRETBOX_SNAPSHOT_DB_PASSWORD=golden-test-password \
//!   go test -run TestSnapshotImportTool
//!
//! 方向 B（Go 导出 → Rust 导入比对）：
//!   SECRETBOX_SNAPSHOT_EXPORT_DB=<目录>/src.db SECRETBOX_SNAPSHOT_EXPORT_PASSWORD=migration-pass-#06 \
//!   SECRETBOX_SNAPSHOT_EXPORT_OUT=<目录>/go-export.secretbox go test -run TestSnapshotExportTool
//!   SECRETBOX_SNAPSHOT_IN=<目录>/go-export.secretbox cargo test -p secretbox-core --test snapshot -- 快照导入

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

/// 方向 A：Rust 导出迁移文件 + 期望明文，供 Go 导入比对（SECRETBOX_SNAPSHOT_OUT 门控）。
#[test]
fn 快照导出供_go_导入() {
    let out_dir = match std::env::var("SECRETBOX_SNAPSHOT_OUT") {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let (db, key, _tmp) = setup();

    let snap = db.get_snapshot().expect("读取快照");
    let content = build_file(&snap, PASSPHRASE).expect("构建迁移文件");
    std::fs::write(
        std::path::Path::new(&out_dir).join("rust-export.secretbox"),
        &content,
    )
    .expect("写出迁移文件");

    // 期望明文（与 Go dump 结构一致）
    let expected = json!({ "salt": db.salt_b64(), "items": dump_plaintext(&db, &key) });
    std::fs::write(
        std::path::Path::new(&out_dir).join("rust-export-expected.json"),
        serde_json::to_string_pretty(&expected).unwrap(),
    )
    .expect("写出期望文件");
}

/// 方向 B：导入 Go 导出的迁移文件并比对黄金样本明文（SECRETBOX_SNAPSHOT_IN 门控）。
#[test]
fn 快照导入_go_导出的文件() {
    let in_path = match std::env::var("SECRETBOX_SNAPSHOT_IN") {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let content = std::fs::read_to_string(&in_path).expect("读取 Go 导出的迁移文件");
    let snap = parse_file(&content, PASSPHRASE).expect("解析迁移文件");

    let mut db = open_fresh();
    db.restore_from_snapshot(&snap).expect("还原快照");
    let key = db.unlock("golden-test-password").expect("原主密码可解锁");

    // 与黄金样本期望逐字段比对（还原后的 id 从 1 重新编号，与 expected.json 一致）
    let expected_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/expected.json");
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(expected_path).unwrap()).unwrap();
    let expected_items = expected["items"].as_array().expect("expected items");

    let actual = dump_plaintext(&db, &key);
    assert_eq!(actual.len(), expected_items.len(), "条目数量不一致");
    for (act, exp) in actual.iter().zip(expected_items.iter()) {
        assert_eq!(act["id"], exp["id"], "id 不一致");
        assert_eq!(act["title"], exp["title"], "标题不一致");
        assert_eq!(act["note"], exp["note"], "备注不一致");
        assert_eq!(act["value"], exp["value"], "内容不一致");
        assert_eq!(act["created_at"], exp["created_at"], "创建时间不一致");
        assert_eq!(act["updated_at"], exp["updated_at"], "更新时间不一致");
        let act_versions = act["versions"].as_array().unwrap();
        let exp_versions = exp["versions"].as_array().unwrap();
        assert_eq!(act_versions.len(), exp_versions.len(), "版本数量不一致");
        for (av, ev) in act_versions.iter().zip(exp_versions.iter()) {
            assert_eq!(av["version"], ev["version"], "版本号不一致");
            assert_eq!(av["snapshot"], ev["snapshot"], "版本快照不一致");
            assert_eq!(av["created_at"], ev["created_at"], "版本时间不一致");
        }
    }
    assert_eq!(db.salt_b64(), Some(expected["salt"].as_str().unwrap().to_string()));
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
