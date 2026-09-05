//! 工单 #05 验收：修改主密码（全量重加密）与跨语言回环。
//!
//! 用法（设置 SECRETBOX_CRUD_OUT 后才执行写盘，普通 cargo test 跳过写盘断言）：
//!   SECRETBOX_CRUD_OUT=<目录> cargo test -p secretbox-core --test password
//! 之后用 Go 测试工具以【新主密码】读回比对（改密后兼容性的硬证据）：
//!   SECRETBOX_VERIFY_DB=<目录>/password.db SECRETBOX_VERIFY_PASSWORD=new-pass-#05 \
//!   SECRETBOX_VERIFY_OUT=<目录>/go-password-dump.json SECRETBOX_VERIFY_EXPECT=<目录>/password-expected.json \
//!   go test -run TestGoldenVerifyDump

use secretbox_core::Db;
use serde_json::json;

const NEW_PASSWORD: &str = "new-pass-#05";

/// 改密后旧密码解锁失败、新密码解锁成功且全部明文与改密前一致。
#[test]
fn 改密后旧密码失效_新密码解锁且数据完整() {
    let (mut db, old_key, _tmp) = setup();

    // 改密前的全部明文
    let before = dump_plaintext(&db, &old_key);
    let old_salt = db.salt_b64().expect("fixture 已设主密码");

    // 改密
    let new_key = db.change_password(&old_key, NEW_PASSWORD).expect("改密成功");

    // 盐值必须已更换
    let new_salt = db.salt_b64().expect("改密后仍应有盐值");
    assert_ne!(new_salt, old_salt, "改密后盐值必须更换");

    // 旧密码解锁失败，新密码解锁成功
    assert!(
        db.unlock("golden-test-password").is_err(),
        "旧密码不应再能解锁"
    );
    db.unlock(NEW_PASSWORD).expect("新密码应能解锁");

    // 数据完整：逐条逐版本与改密前一致
    let after = dump_plaintext(&db, &new_key);
    assert_eq!(after, before, "改密前后全部明文必须一致");
}

/// 写盘 + 供 Go 用新密码读回比对（跨语言回环）。
#[test]
fn 改密后数据库被_go_版用新密码兼容读取() {
    let out_dir = match std::env::var("SECRETBOX_CRUD_OUT") {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let (mut db, old_key, db_path) = setup();
    let before = dump_plaintext(&db, &old_key);

    let new_key = db.change_password(&old_key, NEW_PASSWORD).expect("改密成功");

    // 期望文件：改密后的盐值 + 全部明文
    let expected = json!({ "salt": db.salt_b64(), "items": before });
    std::fs::write(
        std::path::Path::new(&out_dir).join("password-expected.json"),
        serde_json::to_string_pretty(&expected).unwrap(),
    )
    .expect("写出期望文件");

    // 合并 WAL 后复制数据库副本到输出目录
    db.checkpoint_wal().expect("合并 WAL");
    std::fs::copy(db_path, std::path::Path::new(&out_dir).join("password.db")).expect("复制数据库");
    let _ = new_key;
}

/// 复制 fixture 并解锁，返回 (数据库, 旧密码密钥, 数据库路径)。
fn setup() -> (Db, Vec<u8>, std::path::PathBuf) {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden.db");
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-password-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    let db_path = tmp.join("password.db");
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
    let old_key = db.unlock("golden-test-password").expect("解锁 fixture");
    (db, old_key, db_path)
}

/// 导出全部条目（含历史版本快照明文），作为期望数据。
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
