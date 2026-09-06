//! 工单 #01 验收（v2 格式）：修改主密码 = 重新包装 DEK。
//!
//! 改密后旧密码解锁失败、新密码解锁成功、全部明文与改密前一致；
//! 条目密文原样不动（ADR-0003：改密不再触发全库重加密）。

use secretbox_core::Db;

const NEW_PASSWORD: &str = "new-pass-#01";

/// 改密后旧密码失效、新密码解锁成功且数据完整，条目密文不变。
#[test]
fn 改密后旧密码失效_新密码解锁且数据完整() {
    let (mut db, dek, _tmp) = setup();

    // 改密前的全部明文与首条密文
    let before = dump_plaintext(&db, &dek);
    let ciphertext_before = db.get_any_encrypted().expect("已写入条目密文");
    let old_salt = db.salt_b64().expect("已设主密码");

    // 改密：只重新包装 DEK，返回的会话密钥不变
    let returned = db.change_password(&dek, NEW_PASSWORD).expect("改密成功");
    assert_eq!(returned, dek, "DEK 不随改密变化");

    // 盐值必须已更换，条目密文原样不动
    let new_salt = db.salt_b64().expect("改密后仍应有盐值");
    assert_ne!(new_salt, old_salt, "改密后盐值必须更换");
    assert_eq!(
        db.get_any_encrypted(),
        Some(ciphertext_before),
        "改密不得触碰条目密文"
    );

    // 旧密码解锁失败，新密码解锁成功
    assert!(
        db.unlock("old-pass-#01").is_err(),
        "旧密码不应再能解锁"
    );
    db.unlock(NEW_PASSWORD).expect("新密码应能解锁");

    // 数据完整：逐条逐版本与改密前一致
    let after = dump_plaintext(&db, &dek);
    assert_eq!(after, before, "改密前后全部明文必须一致");
}

/// 建一个 v2 全新库并写入两条带历史版本的条目，返回 (库, DEK, 路径)。
fn setup() -> (Db, Vec<u8>, std::path::PathBuf) {
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
    let mut db = Db::open(db_path.to_str().unwrap()).expect("打开新库");
    let dek = db
        .setup_master_password("old-pass-#01")
        .expect("设置主密码");
    db.create_item(&dek, "条目一", "分类", "", "内容一")
        .expect("写入条目一");
    let id = db
        .create_item(&dek, "条目二", "", "备注", "内容二")
        .expect("写入条目二");
    db.update_item(&dek, id, "条目二改", "", "备注改", "内容二改")
        .expect("产生历史版本");
    (db, dek, db_path)
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

use serde_json::json;
