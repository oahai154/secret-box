//! 工单 #01 验收（v2 格式）：DEK 密钥包装架构的核心性质（ADR-0003）。
//!
//! - 会话密钥是随机 DEK，不随主密码变化；
//! - 解包装自带 GCM 校验：空库也能识别错误密码（v1 做不到）；
//! - v2 快照导出→导入回环后凭原主密码解锁。

use secretbox_core::{build_file, generate_recovery_key, normalize_recovery_key, parse_file, Db};

/// 空库 + 错误密码：解包装必须失败（v1 空库无密文可验，错误密码也"解锁成功"）。
#[test]
fn 空库错误密码解锁失败() {
    let (db, _dek, _tmp) = fresh_v2("old-pass-#01");
    assert!(db.unlock("绝对错误的密码").is_err(), "空库也必须校验密码");
}

/// 改密前后解锁得到的是同一个 DEK。
#[test]
fn 会话密钥不随改密变化() {
    let (mut db, dek, _tmp) = fresh_v2("old-pass-#01");
    let key = db.unlock("old-pass-#01").expect("旧密码解锁");
    assert_eq!(key, dek, "解锁返回的会话密钥必须是建库时的 DEK");
    db.change_password(&dek, "new-pass-#01").expect("改密");
    let key2 = db.unlock("new-pass-#01").expect("新密码解锁");
    assert_eq!(key2, dek, "改密后 DEK 保持不变");
    assert_eq!(key2, key);
}

/// v2 快照导出→导入回环：凭原主密码解锁，数据逐字段一致。
#[test]
fn v2快照导出导入回环() {
    let (db, dek, _tmp) = fresh_v2("snap-pass-#01");
    db.create_item(&dek, "条目一", "分类", "", "内容一")
        .expect("写入条目");
    let id = db
        .create_item(&dek, "条目二", "", "备注", "内容二")
        .expect("写入条目二");
    db.update_item(&dek, id, "条目二改", "", "备注改", "内容二改")
        .expect("产生历史版本");

    let before = dump_plaintext(&db, &dek);
    let snap = db.get_snapshot().expect("读取快照");
    assert!(!snap.wrapped_dek_b64.is_empty(), "v2 快照必须携带 DEK 包装");
    let content = build_file(&snap, "迁移口令123").expect("构建迁移文件");
    let parsed = parse_file(&content, "迁移口令123").expect("解析迁移文件");

    let mut db2 = open_fresh();
    db2.restore_from_snapshot(&parsed).expect("还原快照");
    let key2 = db2.unlock("snap-pass-#01").expect("原主密码可解锁");
    assert_eq!(dump_plaintext(&db2, &key2), before, "回环后数据必须一致");
}

/// 恢复密钥侧包装：凭恢复密钥解开同一个 DEK，归一化兜住大小写/分隔符差异。
#[test]
fn 恢复密钥包装与解锁回环() {
    let (mut db, dek, _tmp) = fresh_v2("old-pass-#01");
    assert!(!db.has_recovery_key(), "未设置前应返回 false");

    let code = generate_recovery_key().expect("生成恢复密钥");
    db.set_recovery_key(&code, &dek).expect("设置恢复密钥");
    assert!(db.has_recovery_key());

    // 小写 + 无连字符输入也要通过（抄写差异由归一化兜住）
    let sloppy = normalize_recovery_key(&code).to_lowercase();
    let unwrapped = db.unlock_with_recovery_key(&sloppy).expect("恢复密钥解锁");
    assert_eq!(unwrapped, dek, "恢复密钥必须解开同一个 DEK");

    // 错误恢复密钥被拒
    assert!(db.unlock_with_recovery_key("AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH").is_err());

    // 重置：旧恢复密钥立即作废
    let new_code = generate_recovery_key().expect("再次生成");
    db.set_recovery_key(&new_code, &dek).expect("重置恢复密钥");
    assert!(db.unlock_with_recovery_key(&code).is_err(), "旧恢复密钥必须作废");
    db.unlock_with_recovery_key(&new_code).expect("新恢复密钥可用");
}

/// v2 快照携带双份包装：导出 → 清库 → 主密码、恢复密钥两种凭据分别导入，
/// 数据均逐字段一致（备份不随主密码遗忘而作废，见 ADR-0003）。
#[test]
fn v2快照双凭据导入回环() {
    let (mut db, dek, _tmp) = fresh_v2("snap-pass-#01");
    db.create_item(&dek, "条目一", "分类", "", "内容一")
        .expect("写入条目");
    let id = db
        .create_item(&dek, "条目二", "", "备注", "内容二")
        .expect("写入条目二");
    db.update_item(&dek, id, "条目二改", "", "备注改", "内容二改")
        .expect("产生历史版本");
    let code = generate_recovery_key().expect("生成恢复密钥");
    db.set_recovery_key(&code, &dek).expect("设置恢复密钥");

    let before = dump_plaintext(&db, &dek);
    let snap = db.get_snapshot().expect("读取快照");
    assert!(!snap.wrapped_dek_b64.is_empty(), "主密码侧包装必须存在");
    assert!(!snap.wrapped_dek_recovery_b64.is_empty(), "恢复密钥侧包装必须存在");
    assert!(!snap.recovery_key_enc_b64.is_empty(), "恢复密钥明文加密行必须存在");
    let content = build_file(&snap, "迁移口令123").expect("构建迁移文件");
    let parsed = parse_file(&content, "迁移口令123").expect("解析迁移文件");

    // 凭据 A：主密码导入
    let mut db_a = open_fresh();
    db_a.restore_from_snapshot(&parsed).expect("还原快照");
    let key_a = db_a.unlock("snap-pass-#01").expect("主密码解锁");
    assert_eq!(dump_plaintext(&db_a, &key_a), before, "主密码导入后数据必须一致");
    // 恢复密钥也随快照迁移：查看与解锁均可用
    assert_eq!(
        db_a.get_recovery_key(&key_a).expect("查看恢复密钥"),
        Some(code.clone()),
        "恢复密钥明文加密行必须随快照迁移"
    );

    // 凭据 B：恢复密钥导入（主密码已遗忘场景）
    let mut db_b = open_fresh();
    db_b.restore_from_snapshot(&parsed).expect("还原快照");
    let key_b = db_b.unlock_with_recovery_key(&code).expect("恢复密钥解锁");
    assert_eq!(dump_plaintext(&db_b, &key_b), before, "恢复密钥导入后数据必须一致");
}

/// 已有主密码的库拒绝再次设置——不存在任何覆盖式写入口（v1/v2 皆然）。
#[test]
fn 已设主密码的库拒绝再次设置() {
    // v2 库
    let (mut db, _dek, _tmp) = fresh_v2("old-pass-#01");
    assert!(db.setup_master_password("another-pass").is_err());

    // v1 旧库（黄金样本 fixture）
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/golden.db");
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-dek-guard-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    let copy = tmp.join("golden.db");
    std::fs::copy(fixture, &copy).expect("复制 fixture");
    let mut legacy = Db::open(copy.to_str().unwrap()).expect("打开 v1 库");
    assert!(legacy.setup_master_password("another-pass").is_err());
}

/// 建一个 v2 全新库并设置主密码，返回 (库, DEK, 临时目录)。
fn fresh_v2(password: &str) -> (Db, Vec<u8>, std::path::PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-dek-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp).expect("创建临时目录");
    let mut db = Db::open(tmp.join("dek.db").to_str().unwrap()).expect("打开新库");
    let dek = db.setup_master_password(password).expect("设置主密码");
    (db, dek, tmp)
}

/// 打开一个全新空数据库（未设置主密码）。
fn open_fresh() -> Db {
    let tmp = std::env::temp_dir().join(format!(
        "secretbox-dek-fresh-{}-{}",
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
            versions.push(serde_json::json!({
                "version": v.version,
                "created_at": v.created_at,
                "snapshot": snapshot,
            }));
        }
        items.push(serde_json::json!({
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
