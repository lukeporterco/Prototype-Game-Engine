use std::fs;
use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::app::RenderableKind;

use super::atomic_io::write_bytes_atomic;
use super::compiler::{CompiledEntityDef, SourceLocation};

const MAGIC: &[u8; 4] = b"PGCP";

#[derive(Debug, Clone)]
pub struct ContentPackMeta {
    pub pack_format_version: u16,
    pub compiler_version: String,
    pub game_version: String,
    pub mod_id: String,
    pub mod_load_index: u32,
    pub enabled_mods_hash_sha256_hex: String,
    pub input_hash_sha256_hex: String,
}

#[derive(Debug, Clone)]
pub struct PackedEntityDef {
    pub def_name: String,
    pub label: Option<String>,
    pub renderable: Option<RenderableKind>,
    pub move_speed: Option<f32>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ContentPackV1 {
    pub meta: ContentPackMeta,
    pub records: Vec<PackedEntityDef>,
}

#[derive(Debug, Error)]
pub enum ContentPackError {
    #[error("failed to read/write file {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("pack at {path} has invalid format: {message}")]
    InvalidFormat {
        path: std::path::PathBuf,
        message: String,
    },
}

pub fn write_content_pack_v1(
    path: &Path,
    meta: &ContentPackMeta,
    records: &[CompiledEntityDef],
) -> Result<(), ContentPackError> {
    let mut sorted = records.to_vec();
    sorted.sort_by(|a, b| a.def_name.cmp(&b.def_name));
    let payload = encode_payload(&sorted)?;
    let payload_hash = sha256_bytes(&payload);
    let enabled_hash = hex_to_32(&meta.enabled_mods_hash_sha256_hex, path)?;
    let input_hash = hex_to_32(&meta.input_hash_sha256_hex, path)?;

    let mut bytes = Vec::<u8>::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&meta.pack_format_version.to_le_bytes());
    write_string(&mut bytes, &meta.mod_id, path)?;
    bytes.extend_from_slice(&meta.mod_load_index.to_le_bytes());
    write_string(&mut bytes, &meta.compiler_version, path)?;
    write_string(&mut bytes, &meta.game_version, path)?;
    bytes.extend_from_slice(&enabled_hash);
    bytes.extend_from_slice(&input_hash);
    bytes.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&payload_hash);
    bytes.extend_from_slice(&payload);

    write_bytes_atomic(path, &bytes).map_err(|source| ContentPackError::Io {
        path: path.to_path_buf(),
        source,
    })
}

pub fn read_content_pack_v1(path: &Path) -> Result<ContentPackV1, ContentPackError> {
    let bytes = fs::read(path).map_err(|source| ContentPackError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut cursor = 0usize;

    let magic = read_exact(&bytes, &mut cursor, 4, path)?;
    if magic != MAGIC {
        return Err(invalid_format(path, "invalid magic"));
    }

    let pack_format_version = read_u16(&bytes, &mut cursor, path)?;
    let mod_id = read_string(&bytes, &mut cursor, path)?;
    let mod_load_index = read_u32(&bytes, &mut cursor, path)?;
    let compiler_version = read_string(&bytes, &mut cursor, path)?;
    let game_version = read_string(&bytes, &mut cursor, path)?;
    let enabled_hash = read_exact(&bytes, &mut cursor, 32, path)?;
    let input_hash = read_exact(&bytes, &mut cursor, 32, path)?;
    let def_count = read_u32(&bytes, &mut cursor, path)?;
    let payload_len = read_u32(&bytes, &mut cursor, path)? as usize;
    let expected_payload_hash = read_exact(&bytes, &mut cursor, 32, path)?;
    let payload = read_exact(&bytes, &mut cursor, payload_len, path)?;
    if cursor != bytes.len() {
        return Err(invalid_format(path, "unexpected trailing bytes"));
    }

    let actual_hash = sha256_bytes(payload);
    if expected_payload_hash != actual_hash {
        return Err(invalid_format(path, "payload hash mismatch"));
    }

    let records = decode_payload(payload, def_count as usize, path)?;
    Ok(ContentPackV1 {
        meta: ContentPackMeta {
            pack_format_version,
            compiler_version,
            game_version,
            mod_id,
            mod_load_index,
            enabled_mods_hash_sha256_hex: to_hex_lower(enabled_hash),
            input_hash_sha256_hex: to_hex_lower(input_hash),
        },
        records,
    })
}

fn encode_payload(records: &[CompiledEntityDef]) -> Result<Vec<u8>, ContentPackError> {
    let mut payload = Vec::<u8>::new();
    for record in records {
        write_string(&mut payload, &record.def_name, Path::new("<payload>"))?;
        let mut flags = 0u8;
        if record.label.is_some() {
            flags |= 1 << 0;
        }
        if record.renderable.is_some() {
            flags |= 1 << 1;
        }
        if record.move_speed.is_some() {
            flags |= 1 << 2;
        }
        if record.tags.is_some() {
            flags |= 1 << 3;
        }
        payload.push(flags);

        if let Some(label) = &record.label {
            write_string(&mut payload, label, Path::new("<payload>"))?;
        }
        if let Some(renderable) = record.renderable.clone() {
            let kind = match renderable {
                RenderableKind::Placeholder => 0u8,
                RenderableKind::Sprite(_) => 1u8,
            };
            payload.push(kind);
            if let RenderableKind::Sprite(key) = renderable {
                write_string(&mut payload, &key, path_for_payload())?;
            }
        }
        if let Some(move_speed) = record.move_speed {
            payload.extend_from_slice(&move_speed.to_le_bytes());
        }
        if let Some(tags) = &record.tags {
            if tags.len() > u16::MAX as usize {
                return Err(invalid_format(path_for_payload(), "too many tags"));
            }
            payload.extend_from_slice(&(tags.len() as u16).to_le_bytes());
            for tag in tags {
                write_string(&mut payload, tag, path_for_payload())?;
            }
        }
    }
    Ok(payload)
}

fn decode_payload(
    payload: &[u8],
    expected_count: usize,
    path: &Path,
) -> Result<Vec<PackedEntityDef>, ContentPackError> {
    let mut cursor = 0usize;
    let mut records = Vec::<PackedEntityDef>::with_capacity(expected_count);
    for _ in 0..expected_count {
        let def_name = read_string(payload, &mut cursor, path)?;
        let flags = *read_exact(payload, &mut cursor, 1, path)?
            .first()
            .ok_or_else(|| invalid_format(path, "missing field flags"))?;

        let label = if flags & (1 << 0) != 0 {
            Some(read_string(payload, &mut cursor, path)?)
        } else {
            None
        };
        let renderable = if flags & (1 << 1) != 0 {
            let kind = *read_exact(payload, &mut cursor, 1, path)?
                .first()
                .ok_or_else(|| invalid_format(path, "missing renderable kind"))?;
            Some(match kind {
                0 => RenderableKind::Placeholder,
                1 => RenderableKind::Sprite(read_string(payload, &mut cursor, path)?),
                _ => return Err(invalid_format(path, "invalid renderable kind")),
            })
        } else {
            None
        };
        let move_speed = if flags & (1 << 2) != 0 {
            Some(f32::from_le_bytes(
                read_exact(payload, &mut cursor, 4, path)?
                    .try_into()
                    .map_err(|_| invalid_format(path, "invalid f32 encoding"))?,
            ))
        } else {
            None
        };
        let tags = if flags & (1 << 3) != 0 {
            let count = read_u16(payload, &mut cursor, path)? as usize;
            let mut out = Vec::<String>::with_capacity(count);
            for _ in 0..count {
                out.push(read_string(payload, &mut cursor, path)?);
            }
            Some(out)
        } else {
            None
        };

        records.push(PackedEntityDef {
            def_name,
            label,
            renderable,
            move_speed,
            tags,
        });
    }
    if cursor != payload.len() {
        return Err(invalid_format(path, "payload length mismatch"));
    }
    Ok(records)
}

fn write_string(target: &mut Vec<u8>, value: &str, path: &Path) -> Result<(), ContentPackError> {
    let bytes = value.as_bytes();
    if bytes.len() > u16::MAX as usize {
        return Err(invalid_format(path, "string too long for u16 length"));
    }
    target.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    target.extend_from_slice(bytes);
    Ok(())
}

fn read_string(bytes: &[u8], cursor: &mut usize, path: &Path) -> Result<String, ContentPackError> {
    let len = read_u16(bytes, cursor, path)? as usize;
    let raw = read_exact(bytes, cursor, len, path)?;
    std::str::from_utf8(raw)
        .map(|value| value.to_string())
        .map_err(|_| invalid_format(path, "invalid UTF-8 string in pack"))
}

fn read_u16(bytes: &[u8], cursor: &mut usize, path: &Path) -> Result<u16, ContentPackError> {
    Ok(u16::from_le_bytes(
        read_exact(bytes, cursor, 2, path)?
            .try_into()
            .map_err(|_| invalid_format(path, "invalid u16 encoding"))?,
    ))
}

fn read_u32(bytes: &[u8], cursor: &mut usize, path: &Path) -> Result<u32, ContentPackError> {
    Ok(u32::from_le_bytes(
        read_exact(bytes, cursor, 4, path)?
            .try_into()
            .map_err(|_| invalid_format(path, "invalid u32 encoding"))?,
    ))
}

fn read_exact<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
    path: &Path,
) -> Result<&'a [u8], ContentPackError> {
    let end = cursor.saturating_add(len);
    if end > bytes.len() {
        return Err(invalid_format(path, "unexpected end of file"));
    }
    let out = &bytes[*cursor..end];
    *cursor = end;
    Ok(out)
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn hex_to_32(hex: &str, path: &Path) -> Result<[u8; 32], ContentPackError> {
    let decoded = decode_hex(hex, path)?;
    if decoded.len() != 32 {
        return Err(invalid_format(path, "expected 32-byte hash hex"));
    }
    decoded
        .try_into()
        .map_err(|_| invalid_format(path, "failed converting hash bytes"))
}

fn decode_hex(hex: &str, path: &Path) -> Result<Vec<u8>, ContentPackError> {
    if hex.len() % 2 != 0 {
        return Err(invalid_format(path, "hex string has odd length"));
    }
    let mut out = Vec::<u8>::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let hi =
            from_hex_nibble(bytes[i]).ok_or_else(|| invalid_format(path, "invalid hex digit"))?;
        let lo = from_hex_nibble(bytes[i + 1])
            .ok_or_else(|| invalid_format(path, "invalid hex digit"))?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn from_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn to_hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn invalid_format(path: &Path, message: &str) -> ContentPackError {
    ContentPackError::InvalidFormat {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

fn path_for_payload() -> &'static Path {
    Path::new("<payload>")
}

pub fn compiled_from_packed(
    packed: PackedEntityDef,
    mod_id: &str,
    source_path: &Path,
) -> CompiledEntityDef {
    CompiledEntityDef {
        def_name: packed.def_name,
        label: packed.label,
        renderable: packed.renderable,
        move_speed: packed.move_speed,
        tags: packed.tags,
        source_mod_id: mod_id.to_string(),
        source_file_path: source_path.to_path_buf(),
        source_location: None::<SourceLocation>,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::content::manifest::CONTENT_PACK_FORMAT_VERSION;

    #[test]
    fn pack_roundtrip_preserves_records() {
        let temp = TempDir::new().expect("temp");
        let path = temp.path().join("a.pack");
        let meta = ContentPackMeta {
            pack_format_version: CONTENT_PACK_FORMAT_VERSION,
            compiler_version: "1".to_string(),
            game_version: "1".to_string(),
            mod_id: "base".to_string(),
            mod_load_index: 0,
            enabled_mods_hash_sha256_hex: "00".repeat(32),
            input_hash_sha256_hex: "11".repeat(32),
        };
        let records = vec![CompiledEntityDef {
            def_name: "proto.player".to_string(),
            label: Some("Player".to_string()),
            renderable: Some(RenderableKind::Sprite("player".to_string())),
            move_speed: Some(5.0),
            tags: Some(vec!["colonist".to_string()]),
            source_mod_id: "base".to_string(),
            source_file_path: Path::new("defs.xml").to_path_buf(),
            source_location: None,
        }];
        write_content_pack_v1(&path, &meta, &records).expect("write");
        let loaded = read_content_pack_v1(&path).expect("read");
        assert_eq!(loaded.meta.mod_id, "base");
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.records[0].def_name, "proto.player");
        assert_eq!(
            loaded.records[0].renderable,
            Some(RenderableKind::Sprite("player".to_string()))
        );
        assert_eq!(loaded.records[0].tags, Some(vec!["colonist".to_string()]));
    }
}
