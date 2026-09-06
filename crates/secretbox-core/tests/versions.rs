//! 工单 #04 验收：历史版本恢复/删除的核心语义。
//!
//! 在 v1 黄金样本副本上验证（同时覆盖"v1 只读导入源"的读路径，见 ADR-0003）。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use secretbox_core::Db;

/// 不写盘的常规断言（恢复产生新版本、删除版本生效、删除需验证的密码由命令层把关）。
#[test]
fn 恢复与删除版本的核心语义() {
    let (db, key, _tmp) = setup();
    run_history_flow(&db, &key);
}

/// 复制 fixture 并解锁，返回 (数据库, 密钥, 数据库路径)。
fn setup() -> (Db, Vec<u8>, std::path::PathBuf) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden.db");
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-versions-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    let db_path = tmp.join("versions.db");
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
    let salt_b64 = db.salt_b64().expect("fixture 已设主密码");
    let (key, _) =
        secretbox_core::derive_key("golden-test-password", &BASE64.decode(salt_b64).unwrap())
            .expect("派生密钥");
    (db, key, db_path)
}

/// 历史版本核心流程：改两次产生版本 → 删一个版本 → 恢复一个版本。
fn run_history_flow(db: &Db, key: &[u8]) {
    // 公司邮箱（id=2）在 fixture 中有 3 个版本
    assert_eq!(db.list_versions(2).unwrap().len(), 3);

    // 修改一次 → 版本 4
    let it = db.get_item(key, 2).unwrap();
    let v4 = db
        .update_item(key, 2, &it.title, &it.category, &it.note, "v4-content")
        .unwrap();
    assert_eq!(v4.value, "v4-content");
    assert_eq!(db.list_versions(2).unwrap().len(), 4);

    // 删除版本 3
    db.delete_version(2, 3).expect("删除版本 3");
    assert_eq!(db.list_versions(2).unwrap().len(), 3);
    assert!(db.get_version_snapshot(key, 2, 3).is_err(), "版本 3 应已删除");

    // 重复删除同一版本 → 报"版本不存在"
    assert!(db.delete_version(2, 3).is_err());

    // 恢复版本 2 的快照 → 当前内容回到 v2 内容，且产生新版本 5
    let v2_snapshot = db.get_version_snapshot(key, 2, 2).unwrap();
    let restored = db.restore_version(key, 2, 2).unwrap();
    assert_eq!(restored.value, v2_snapshot);
    let versions = db.list_versions(2).unwrap();
    assert_eq!(versions.len(), 4, "恢复应产生新版本");
    assert_eq!(versions[0].version, 5, "新版本号应为 5");
}
