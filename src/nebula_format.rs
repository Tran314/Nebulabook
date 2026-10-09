//! Portable, password-free obfuscation, NOT confidentiality or an attacker-proof signature.
//!
//! The format key is deliberately public. Anyone with this source can
//! decrypt or produce a valid file. See NEBULA_FORMAT.md for the exact protocol.
use crate::model::Notebook;
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"NEBULA\r\n";
pub const HEADER_BYTES: usize = 44;
pub const TAG_BYTES: usize = 16;
pub const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = MAX_PAYLOAD_BYTES + HEADER_BYTES as u64 + TAG_BYTES as u64;
const PUBLIC_FORMAT_KEY: &[u8; 32] = b"Nebulabook public format key v1!";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    notebook: Notebook,
    #[serde(deserialize_with = "required_source_digest")]
    // Reserved v1 envelope field; accepted but never used to access another file.
    legacy_source_sha256: Option<String>,
}

fn required_source_digest<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Encode the current native format with an independent random nonce.
pub fn encode(notebook: &Notebook) -> Result<Vec<u8>, String> {
    let mut nonce = [0_u8; 24];
    getrandom::fill(&mut nonce)
        .map_err(|error| format!("无法获取系统安全随机数，未保存：{error}"))?;
    encode_with_nonce(notebook, nonce)
}

fn encode_with_nonce(notebook: &Notebook, nonce: [u8; 24]) -> Result<Vec<u8>, String> {
    notebook.validate()?;
    // Serialize borrowed data rather than cloning an entire notebook.
    #[derive(Serialize)]
    struct PayloadRef<'a> {
        notebook: &'a Notebook,
        legacy_source_sha256: Option<&'a str>,
    }
    let plaintext = serde_json::to_vec(&PayloadRef {
        notebook,
        legacy_source_sha256: None,
    })
    .map_err(|error| format!("无法编码 .nebula 数据：{error}"))?;
    if plaintext.len() as u64 > MAX_PAYLOAD_BYTES {
        return Err(".nebula 数据超过 64 MiB 安全上限。".into());
    }
    let mut header = Vec::with_capacity(HEADER_BYTES);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&1_u16.to_le_bytes());
    header.extend_from_slice(&0_u16.to_le_bytes());
    header.extend_from_slice(&nonce);
    header.extend_from_slice(&((plaintext.len() + TAG_BYTES) as u64).to_le_bytes());
    let cipher = XChaCha20Poly1305::new(PUBLIC_FORMAT_KEY.into());
    let ciphertext = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &plaintext,
                aad: &header,
            },
        )
        .map_err(|_| "无法封装 .nebula 数据，未保存。".to_string())?;
    header.extend_from_slice(&ciphertext);
    Ok(header)
}

pub fn decode(bytes: &[u8]) -> Result<Notebook, String> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(".nebula 文件超过 64 MiB 载荷安全上限。".into());
    }
    if bytes.len() < HEADER_BYTES + TAG_BYTES || &bytes[..8] != MAGIC {
        return Err("无效或截断的 .nebula 文件头。".into());
    }
    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    if version != 1 {
        return Err(format!(
            "不支持的 .nebula 格式版本 {version}；请使用兼容版本，原文件未修改。"
        ));
    }
    if bytes[10..12] != [0, 0] {
        return Err("不支持的 .nebula 标志位。".into());
    }
    let length = u64::from_le_bytes(bytes[36..44].try_into().unwrap());
    if length < TAG_BYTES as u64
        || length > MAX_PAYLOAD_BYTES + TAG_BYTES as u64
        || length != (bytes.len() - HEADER_BYTES) as u64
    {
        return Err(".nebula 长度校验失败；文件可能损坏、截断或被修改。".into());
    }
    let nonce: [u8; 24] = bytes[12..36].try_into().unwrap();
    let cipher = XChaCha20Poly1305::new(PUBLIC_FORMAT_KEY.into());
    let plaintext = cipher
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &bytes[HEADER_BYTES..],
                aad: &bytes[..HEADER_BYTES],
            },
        )
        .map_err(|_| {
            ".nebula 完整性校验失败；文件可能损坏或被修改，未读取任何笔记。".to_string()
        })?;
    let value = crate::import_export::unique_json(&plaintext)?;
    let snapshot: Snapshot = serde_json::from_value(value)
        .map_err(|error| format!("无效的 .nebula 数据结构：{error}"))?;
    validate_source_digest(snapshot.legacy_source_sha256.as_deref())?;
    snapshot.notebook.validate()?;
    Ok(snapshot.notebook)
}

fn validate_source_digest(value: Option<&str>) -> Result<(), String> {
    if value.is_some_and(|value| {
        value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err("无效的旧数据来源校验值。".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Note;

    #[test]
    fn unicode_round_trip_is_obfuscated_and_randomized() {
        let mut notebook = Notebook::default();
        notebook.notes.push(Note::new(
            "中文标题📝".into(),
            "第一行\n<script>inert</script>".into(),
        ));
        let first = encode(&notebook).unwrap();
        let second = encode(&notebook).unwrap();
        assert_ne!(first, second);
        assert_eq!(decode(&first).unwrap(), notebook);
        assert!(!first
            .windows("中文标题".len())
            .any(|part| part == "中文标题".as_bytes()));
    }

    #[test]
    fn rejects_every_single_byte_modification_and_truncation() {
        let good = encode(&Notebook::default()).unwrap();
        for offset in 0..good.len() {
            let mut changed = good.clone();
            changed[offset] ^= 1;
            assert!(decode(&changed).is_err(), "accepted modified byte {offset}");
            assert!(
                decode(&good[..offset]).is_err(),
                "accepted truncated size {offset}"
            );
        }
        let mut appended = good.clone();
        appended.push(b'\n');
        assert!(decode(&appended).is_err());
        let mut unsupported = good;
        unsupported[8] = 99;
        assert!(decode(&unsupported).unwrap_err().contains("版本 99"));
    }

    #[test]
    fn deterministic_fixture_matches_public_protocol() {
        let bytes = encode_with_nonce(
            &Notebook::default(),
            std::array::from_fn(|index| index as u8),
        )
        .unwrap();
        assert_eq!(bytes, include_bytes!("../tests/fixtures/empty-v1.nebula"));
        assert_eq!(decode(&bytes).unwrap(), Notebook::default());
    }

    fn seal_payload(raw: &str) -> Vec<u8> {
        let nonce = [7_u8; 24];
        let mut header = MAGIC.to_vec();
        header.extend_from_slice(&1_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&nonce);
        header.extend_from_slice(&((raw.len() + TAG_BYTES) as u64).to_le_bytes());
        let cipher = XChaCha20Poly1305::new(PUBLIC_FORMAT_KEY.into());
        let ciphertext = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: raw.as_bytes(),
                    aad: &header,
                },
            )
            .unwrap();
        header.extend_from_slice(&ciphertext);
        header
    }

    #[test]
    fn existing_v1_source_field_is_inert_and_new_saves_use_null() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notebook.nebula");
        let old_json = path.with_extension("json");
        std::fs::write(&old_json, b"unrelated old JSON").unwrap();
        let notebook = Notebook {
            legacy_archives: vec![serde_json::json!({"preserved": "inert native metadata"})],
            ..Notebook::default()
        };
        let raw = serde_json::json!({
            "notebook": notebook,
            "legacy_source_sha256": "a".repeat(64),
        })
        .to_string();
        let previous = seal_payload(&raw);
        std::fs::write(&path, &previous).unwrap();
        let (mut storage, mut loaded) = crate::storage::Storage::open(path.clone()).unwrap();
        assert_eq!(loaded, notebook);
        storage.save(&loaded).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        loaded
            .notes
            .push(Note::new("Current note".into(), "New content".into()));
        storage.save(&loaded).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(decode(&saved).unwrap(), loaded);
        let nonce: [u8; 24] = saved[12..36].try_into().unwrap();
        let plaintext = XChaCha20Poly1305::new(PUBLIC_FORMAT_KEY.into())
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &saved[HEADER_BYTES..],
                    aad: &saved[..HEADER_BYTES],
                },
            )
            .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
        assert!(envelope["legacy_source_sha256"].is_null());
        assert_eq!(std::fs::read(&old_json).unwrap(), b"unrelated old JSON");
        assert_eq!(std::fs::read(storage.backup_path()).unwrap(), previous);
    }

    #[test]
    fn authenticated_but_invalid_payloads_still_fail_closed() {
        let valid_notebook = r#"{"schema_version":1,"notes":[],"folders":[],"tags":[]}"#;
        for raw in [
            format!(r#"{{"notebook":{valid_notebook}}}"#),
            format!(r#"{{"notebook":{valid_notebook},"legacy_source_sha256":null,"unknown":true}}"#),
            format!(r#"{{"notebook":{valid_notebook},"legacy_source_sha256":"bad-hash"}}"#),
            r#"{"notebook":{"schema_version":99,"notes":[],"folders":[],"tags":[]},"legacy_source_sha256":null}"#.into(),
            r#"{"notebook":{"schema_version":1,"notes":[],"notes":[],"folders":[],"tags":[]},"legacy_source_sha256":null}"#.into(),
        ] {
            assert!(decode(&seal_payload(&raw)).is_err(), "accepted invalid authenticated payload: {raw}");
        }
    }
}
