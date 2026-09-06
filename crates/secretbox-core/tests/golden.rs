//! v1 黄金样本测试：scrypt 派生向量 + v1 旧库只读读路径（见 ADR-0003）。
//!
//! fixture 由原 Go 版自身代码生成，使用公开测试密码，不含真实数据。
//! v2 起 Go 版兼容已废止（ADR-0003），此文件保留两项职责：
//! ① KDF/密文布局向量锁定加密原语不回归；② v1 只读解锁路径的读语义回归。

use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use secretbox_core::{decrypt, derive_key, Db};
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenExpected {
    password: String,
    salt: String,
    kdf_vector: GoldenKdfVector,
    decrypt_vectors: Vec<GoldenDecryptVector>,
    items: Vec<GoldenItem>,
}

#[derive(Deserialize)]
struct GoldenKdfVector {
    password: String,
    salt_hex: String,
    key_hex: String,
}

#[derive(Deserialize)]
struct GoldenDecryptVector {
    plaintext: String,
    cipher: String,
}

#[derive(Deserialize)]
struct GoldenItem {
    id: i64,
    title: String,
    category: String,
    note: String,
    created_at: String,
    updated_at: String,
    value: String,
    version_count: i64,
    versions: Vec<GoldenVersion>,
}

#[derive(Deserialize)]
struct GoldenVersion {
    version: i64,
    created_at: String,
    snapshot: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load_expected() -> GoldenExpected {
    let path = fixtures_dir().join("expected.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("读取 {}: {err}", path.display()));
    serde_json::from_str(&raw).expect("expected.json 格式有效")
}

/// 把 golden.db 复制到临时目录再打开，避免测试在 fixtures 目录留下 -wal/-shm。
fn open_fixture_copy() -> (Db, tempfile_guard::TempDir) {
    let tmp = tempfile_guard::TempDir::new();
    let db_copy = tmp.path().join("golden.db");
    std::fs::copy(fixtures_dir().join("golden.db"), &db_copy).expect("复制 golden.db");
    let db = Db::open(db_copy.to_str().unwrap()).expect("打开 golden.db 副本");
    (db, tmp)
}

/// 极简临时目录：测试结束时整体删除。
mod tempfile_guard {
    use std::path::PathBuf;
    use std::process;

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new() -> TempDir {
            let path = std::env::temp_dir().join(format!(
                "secretbox-golden-test-{}-{}",
                process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("系统时钟正常")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).expect("创建临时目录");
            TempDir { path }
        }

        pub fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[test]
fn kdf_派生与_go_版一致() {
    let expected = load_expected();
    let salt: Vec<u8> = hex_decode(&expected.kdf_vector.salt_hex);
    let (key, _) = derive_key(&expected.kdf_vector.password, &salt)
        .expect("固定盐派生密钥应成功");
    assert_eq!(hex_encode(&key), expected.kdf_vector.key_hex);
}

#[test]
fn 能解密_go_版加密的全部向量() {
    let expected = load_expected();
    let salt = BASE64
        .decode(&expected.salt)
        .expect("盐值 base64 有效");
    let (key, _) = derive_key(&expected.password, &salt).expect("派生密钥应成功");
    for vector in &expected.decrypt_vectors {
        let plain = decrypt(&key, &vector.cipher)
            .unwrap_or_else(|err| panic!("解密向量失败: {err}"));
        assert_eq!(plain, vector.plaintext);
    }
}

#[test]
fn 错误密码解密失败() {
    let expected = load_expected();
    let salt = BASE64.decode(&expected.salt).expect("盐值 base64 有效");
    let (key, _) = derive_key("绝对错误的密码", &salt).expect("派生密钥应成功");
    let err = decrypt(&key, &expected.decrypt_vectors[1].cipher)
        .expect_err("错误密码必须解密失败");
    assert!(err.to_string().contains("主密码可能不正确"));
}

#[test]
fn 打开黄金样本库并逐字段比对() {
    let expected = load_expected();
    let (db, _tmp) = open_fixture_copy();

    assert!(db.has_master_password(), "golden.db 应已设置主密码");

    // 解锁成功，派生密钥可解密全部条目
    let key = db.unlock(&expected.password).expect("正确密码应解锁成功");

    // 错误密码解锁失败
    assert!(db.unlock("错误的密码").is_err(), "错误密码必须解锁失败");

    let items = db.list_items().expect("列出条目");
    assert_eq!(items.len(), expected.items.len(), "条目数量一致");

    // expected.json 按 id 升序生成；list_items 按 updated_at 排序，因此按 id 建索引比对
    let mut expected_by_id: Vec<&GoldenItem> = expected.items.iter().collect();
    expected_by_id.sort_by_key(|item| item.id);

    for item in &items {
        let want = expected_by_id
            .iter()
            .find(|want| want.id == item.id)
            .unwrap_or_else(|| panic!("出现意外条目 id={}", item.id));
        assert_eq!(item.title, want.title, "条目 {} 标题", item.id);
        assert_eq!(item.category, want.category, "条目 {} 分类", item.id);
        assert_eq!(item.note, want.note, "条目 {} 备注", item.id);
        assert_eq!(item.created_at, want.created_at, "条目 {} 创建时间", item.id);
        assert_eq!(item.updated_at, want.updated_at, "条目 {} 更新时间", item.id);
        assert_eq!(
            item.version_count, want.version_count,
            "条目 {} 版本数",
            item.id
        );

        // 单条读取的明文
        let full = db.get_item(&key, item.id).expect("读取单条");
        assert_eq!(full.value, want.value, "条目 {} 明文", item.id);

        // 历史版本逐字段比对（两侧都按版本号倒序返回，直接对应）
        let versions = db.list_versions(item.id).expect("列出历史版本");
        assert_eq!(versions.len(), want.versions.len(), "条目 {} 版本列表", item.id);
        for (got, want_version) in versions.iter().zip(&want.versions) {
            assert_eq!(got.version, want_version.version);
            assert_eq!(got.created_at, want_version.created_at);
            let snapshot = db
                .get_version_snapshot(&key, item.id, got.version)
                .expect("读取版本快照");
            assert_eq!(snapshot, want_version.snapshot, "条目 {} 版本 {} 快照明文", item.id, got.version);
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("十六进制有效"))
        .collect()
}
