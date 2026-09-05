//! 工单 #03 验收：Rust 写路径与 Go 版跨语言回环。
//!
//! 用法（设置 SECRETBOX_CRUD_OUT 后才执行写盘，普通 cargo test 跳过）：
//!   SECRETBOX_CRUD_OUT=<目录> cargo test -p secretbox-core --test crud
//! 之后用 Go 测试工具读回比对：
//!   SECRETBOX_VERIFY_DB=<目录>/crud.db SECRETBOX_VERIFY_OUT=<目录>/go-dump.json \
//!   SECRETBOX_VERIFY_EXPECT=<目录>/crud-expected.json SECRETBOX_VERIFY_PASSWORD=golden-test-password \
//!   go test -run TestGoldenVerifyDump
//!
//! Rust 对 fixture 副本做增/改/删后，用读路径导出全部明文作为期望值；
//! Go 独立解密同一数据库并逐字段比对——Rust 写入的密文必须能被 Go 解开。

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use secretbox_core::Db;
use serde_json::json;

#[test]
fn crud_写入后数据库仍被_go_版兼容读取() {
    let out_dir = match std::env::var("SECRETBOX_CRUD_OUT") {
        Ok(dir) => dir,
        Err(_) => return, // 未设置输出目录时跳过写盘
    };

    // 1. 复制 fixture 副本
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/golden.db"
    );
    let db_path = std::path::Path::new(&out_dir).join("crud.db");
    let _ = std::fs::remove_file(&db_path);
    std::fs::copy(fixture, &db_path).expect("复制 golden.db");
    let db = Db::open(db_path.to_str().unwrap()).expect("打开副本");

    // 2. 用库中盐值派生密钥（与 unlock 相同路径）并解锁
    let salt_b64 = db.salt_b64().expect("fixture 已设主密码");
    let (key, _) =
        secretbox_core::derive_key("golden-test-password", &BASE64.decode(salt_b64).unwrap())
            .expect("派生密钥");
    assert!(db.unlock("golden-test-password").is_ok());

    // 3. 删除一个黄金条目（家里 Wi-Fi，id=3）
    db.delete_item(3).expect("删除条目");

    // 4. 修改黄金条目 GitHub（id=1）的内容 → 产生版本 2
    let github = db.get_item(&key, 1).expect("读取 GitHub");
    let updated = db
        .update_item(&key, 1, &github.title, &github.category, &github.note, "rotated-pass-2026")
        .expect("更新 GitHub");
    assert_eq!(updated.value, "rotated-pass-2026");

    // 5. 新增条目（含中文、emoji、换行、空分类）
    let new_id = db
        .create_item(&key, "测试站点 🧪", "", "临时备注\n第二行", "user: pass 🦀")
        .expect("新增条目");
    let _ = db
        .update_item(&key, new_id, "测试站点 🧪", "开发", "改过备注", "user: new-pass")
        .expect("更新新条目");

    // 6. 用读路径导出全部明文作为期望值（Rust 视角的库内容）
    let mut items = Vec::new();
    for it in db.list_items().expect("列出条目") {
        let full = db.get_item(&key, it.id).expect("读取条目");
        let mut versions = Vec::new();
        for v in db.list_versions(it.id).expect("列出版本") {
            let snapshot = db
                .get_version_snapshot(&key, it.id, v.version)
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

    let expected = json!({
        "salt": db.salt_b64(),
        "items": items,
    });
    std::fs::write(
        std::path::Path::new(&out_dir).join("crud-expected.json"),
        serde_json::to_string_pretty(&expected).unwrap(),
    )
    .expect("写出期望文件");

    // 自检：期望条目数 = 3（原3 - 删1 + 新1）
    assert_eq!(expected["items"].as_array().map(|a| a.len()), Some(3));
}
