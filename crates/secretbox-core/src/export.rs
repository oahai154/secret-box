//! 明文导出（ADR-0004）：把全部条目（仅当前版本，不含历史版本）生成不加密的 UTF-8 CSV。
//!
//! 与 migration.rs 的快照导出分工：快照是可还原的加密备份，明文导出是
//! 单程迁出，不设对应的导入路径。CSV 必须带 BOM（Windows Excel 打开
//! 无 BOM 的 UTF-8 会乱码），转义遵循 RFC 4180：含逗号/引号/CR/LF 的
//! 字段用引号包裹，内部引号翻倍；行结束符统一 CRLF。

use chrono::Local;

use crate::db::Item;

/// 建议的导出文件名：secretbox-plain-<时间戳>.csv，与快照的 secretbox-backup-*.secretbox 区分。
pub fn plaintext_filename() -> String {
    format!("secretbox-plain-{}.csv", Local::now().format("%Y%m%d-%H%M%S"))
}

/// RFC 4180 字段转义：仅当字段含逗号、引号、CR、LF 时加引号包裹。
fn escape_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\r') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// 生成带 BOM 的 CSV 文本：表头"标题,分类,内容,备注"，每行一个条目，CRLF 结尾。
pub fn build_csv(items: &[Item]) -> String {
    let mut out = String::from("\u{FEFF}标题,分类,内容,备注\r\n");
    for it in items {
        let row = [
            it.title.as_str(),
            it.category.as_str(),
            it.value.as_str(),
            it.note.as_str(),
        ]
        .map(escape_field)
        .join(",");
        out.push_str(&row);
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, category: &str, value: &str, note: &str) -> Item {
        Item {
            id: 1,
            title: title.into(),
            category: category.into(),
            note: note.into(),
            created_at: "2026-01-01T00:00:00+08:00".into(),
            updated_at: "2026-01-01T00:00:00+08:00".into(),
            value: value.into(),
            version_count: 0,
        }
    }

    #[test]
    fn 空列表只有表头且带BOM() {
        let csv = build_csv(&[]);
        assert!(csv.starts_with('\u{FEFF}'), "必须带 UTF-8 BOM，否则 Windows Excel 打开乱码");
        assert_eq!(csv.trim_start_matches('\u{FEFF}'), "标题,分类,内容,备注\r\n");
    }

    #[test]
    fn 普通字段不加引号() {
        let csv = build_csv(&[item("GitHub", "账号密码", "ghp_xxx", "工作用")]);
        assert_eq!(
            csv.trim_start_matches('\u{FEFF}'),
            "标题,分类,内容,备注\r\nGitHub,账号密码,ghp_xxx,工作用\r\n"
        );
    }

    #[test]
    fn 含逗号引号换行的字段被引号包裹() {
        let csv = build_csv(&[item("a,b", "", "密码\"123\"\n第二行", "")]);
        assert_eq!(
            csv.trim_start_matches('\u{FEFF}'),
            "标题,分类,内容,备注\r\n\"a,b\",,\"密码\"\"123\"\"\n第二行\",\r\n"
        );
    }

    #[test]
    fn 文件名区分于快照且带csv扩展名() {
        let name = plaintext_filename();
        assert!(name.starts_with("secretbox-plain-"));
        assert!(name.ends_with(".csv"));
    }
}
