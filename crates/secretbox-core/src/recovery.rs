//! 恢复密钥：生成、归一化与 DEK 解包装（见 ADR-0003）。
//!
//! 恢复密钥是 160 位随机数的 base32 分组码（8 组 × 4 字符，连字符分隔）。
//! 字符集去除易混淆的 0/O/1/I；校验与解锁前一律归一化（大写、去分隔符），
//! 用户抄写时的大小写与空格差异不影响验证。

use crate::crypto::{self, CryptoError};

/// base32 字符集：2-9 + A-Z 去掉 I/O，共 32 个无歧义字符。
const ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";

/// 随机字节数：20 字节 = 160 位熵（高于 128 位要求）。
const RANDOM_BYTES: usize = 20;
/// 每组字符数。
const GROUP_SIZE: usize = 4;

/// 生成一个随机恢复密钥，形如 `XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX`。
pub fn generate_recovery_key() -> Result<String, CryptoError> {
    let bytes = crypto::random_bytes(RANDOM_BYTES)?;
    Ok(format_grouped(&encode_base32(&bytes)))
}

/// base32 编码：每 5 字节 → 8 字符（长度必须是 5 的倍数）。
fn encode_base32(bytes: &[u8]) -> String {
    assert!(bytes.len().is_multiple_of(5), "恢复密钥编码要求 5 字节对齐");
    let mut out = String::with_capacity(bytes.len() * 8 / 5);
    for chunk in bytes.chunks(5) {
        // 5 字节按大端拼成 40 bit，逐 5 bit 取出
        let bits = u64::from_be_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], 0, 0, 0,
        ]) >> 24;
        for i in 0..8 {
            let index = ((bits >> (35 - i * 5)) & 0x1f) as usize;
            out.push(ALPHABET[index] as char);
        }
    }
    out
}

/// 归一化恢复密钥：大写、只保留字母数字（去连字符/空格）。
pub fn normalize_recovery_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// 把归一化后的 32 字符码按 4 字符分组（用于展示与存盘的规范形态）。
pub fn format_grouped(normalized: &str) -> String {
    normalized
        .chars()
        .collect::<Vec<_>>()
        .chunks(GROUP_SIZE)
        .map(|g| g.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 生成的恢复密钥格式正确() {
        let key = generate_recovery_key().expect("生成成功");
        let chars: String = key.chars().filter(|c| *c != '-').collect();
        assert_eq!(chars.len(), 32, "总字符数 32");
        assert_eq!(key.matches('-').count(), 7, "8 组共 7 个连字符");
        for group in key.split('-') {
            assert_eq!(group.len(), GROUP_SIZE, "每组 4 字符");
        }
        assert!(
            chars
                .bytes()
                .all(|c| ALPHABET.contains(&c.to_ascii_uppercase())),
            "字符集不含 0/O/1/I: {key}"
        );
        let other = generate_recovery_key().expect("再次生成");
        assert_ne!(key, other, "随机生成不应重复");
    }

    #[test]
    fn 编码可无损解码回原字节() {
        // 测试内自带解码器，验证编码没有任何位错位
        let decode = |code: &str| -> Vec<u8> {
            let mut bytes = Vec::new();
            for group in code.as_bytes().chunks(8) {
                let mut bits: u64 = 0;
                for &c in group {
                    let index = ALPHABET
                        .iter()
                        .position(|&a| a == c)
                        .expect("字符必须在字母表内");
                    bits = (bits << 5) | index as u64;
                }
                // 40 bit → 5 字节，大端
                for shift in [32u64, 24, 16, 8, 0] {
                    bytes.push(((bits >> shift) & 0xff) as u8);
                }
            }
            bytes
        };
        for vector in [
            vec![0u8; 20],
            vec![0xffu8; 20],
            (0..20).collect::<Vec<u8>>(),
            vec![0xa5, 0x5a, 0xf0, 0x0f, 0xcc, 0x33, 0x81, 0x7e, 0x3c, 0xc3, 0xde, 0xed, 0x01, 0xfe, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc],
        ] {
            let encoded = encode_base32(&vector);
            assert_eq!(decode(&encoded), vector, "向量 {vector:?} 编码应可无损解码");
        }
    }

    #[test]
    fn 归一化忽略大小写与分隔符() {
        let key = generate_recovery_key().expect("生成成功");
        assert_eq!(normalize_recovery_key(&key), normalize_recovery_key(&key.to_lowercase()));
        assert_eq!(
            normalize_recovery_key("abcd-efgh jklm"),
            "ABCDEFGHJKLM".to_string().to_uppercase()
        );
    }
}
