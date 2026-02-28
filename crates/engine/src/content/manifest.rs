use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::atomic_io::write_text_atomic;
use super::pack::ContentPackError;
use super::types::ContentPlanError;

pub(crate) const CONTENT_PACK_FORMAT_VERSION: u16 = 3;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ManifestV1 {
    pub pack_format_version: u16,
    pub compiler_version: String,
    pub game_version: String,
    pub mod_id: String,
    pub mod_load_index: u32,
    pub enabled_mods_hash_sha256_hex: String,
    pub input_hash_sha256_hex: String,
}

#[derive(Debug, Clone)]
pub(crate) enum ManifestReadState {
    Missing,
    Unreadable,
    Present(ManifestV1),
}

pub(crate) fn read_manifest(path: &Path) -> Result<ManifestReadState, ContentPlanError> {
    if !path.exists() {
        return Ok(ManifestReadState::Missing);
    }

    let raw = fs::read_to_string(path).map_err(|source| ContentPlanError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let parsed = match serde_json::from_str::<ManifestV1>(&raw) {
        Ok(value) => value,
        Err(_) => return Ok(ManifestReadState::Unreadable),
    };
    Ok(ManifestReadState::Present(parsed))
}

pub(crate) fn content_pack_cache_dir(cache_dir: &Path) -> PathBuf {
    cache_dir.join("content_packs")
}

pub(crate) fn pack_path(cache_dir: &Path, mod_id: &str) -> PathBuf {
    content_pack_cache_dir(cache_dir).join(format!("{mod_id}.pack"))
}

pub(crate) fn manifest_path(cache_dir: &Path, mod_id: &str) -> PathBuf {
    content_pack_cache_dir(cache_dir).join(format!("{mod_id}.manifest.json"))
}

pub(crate) fn write_manifest_atomic(
    path: &Path,
    manifest: &ManifestV1,
) -> Result<(), ContentPackError> {
    let text =
        serde_json::to_string(manifest).map_err(|error| ContentPackError::InvalidFormat {
            path: path.to_path_buf(),
            message: format!("failed to encode manifest json: {error}"),
        })?;
    write_text_atomic(path, &text).map_err(|source| ContentPackError::Io {
        path: path.to_path_buf(),
        source,
    })
}
