use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::types::ContentPlanError;

#[derive(Debug, Clone)]
pub(crate) struct ModInputHash {
    pub xml_file_count: usize,
    pub hash_hex: String,
}

pub(crate) fn hash_enabled_mods_list(mod_ids_in_order: &[String]) -> String {
    let mut hasher = Sha256::new();
    for mod_id in mod_ids_in_order {
        hasher.update(mod_id.as_bytes());
        hasher.update([0u8]);
    }
    to_hex_lower(&hasher.finalize())
}

pub(crate) fn hash_mod_xml_inputs(mod_dir: &Path) -> Result<ModInputHash, ContentPlanError> {
    let xml_files = collect_xml_files(mod_dir)?;
    let mut hasher = Sha256::new();
    for (normalized_rel, abs_path) in &xml_files {
        let bytes = fs::read(abs_path).map_err(|source| ContentPlanError::ReadFile {
            path: abs_path.clone(),
            source,
        })?;
        hasher.update(normalized_rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(&bytes);
    }

    Ok(ModInputHash {
        xml_file_count: xml_files.len(),
        hash_hex: to_hex_lower(&hasher.finalize()),
    })
}

fn collect_xml_files(mod_dir: &Path) -> Result<Vec<(String, PathBuf)>, ContentPlanError> {
    let mut files = Vec::<(String, PathBuf)>::new();
    collect_recursive(mod_dir, mod_dir, &mut files)?;
    files.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(files)
}

fn collect_recursive(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
) -> Result<(), ContentPlanError> {
    let entries = fs::read_dir(current).map_err(|source| ContentPlanError::ReadDir {
        path: current.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| ContentPlanError::ReadDirEntry {
            path: current.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_recursive(root, &path, files)?;
            continue;
        }
        if !is_xml_file(&path) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("path discovered under root")
            .to_path_buf();
        let normalized = normalize_rel_path(&rel);
        files.push((normalized, path));
    }
    Ok(())
}

fn is_xml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
}

fn normalize_rel_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn to_hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn enabled_mods_hash_order_sensitive() {
        let a = hash_enabled_mods_list(&["base".to_string(), "a".to_string(), "b".to_string()]);
        let b = hash_enabled_mods_list(&["base".to_string(), "b".to_string(), "a".to_string()]);
        assert_ne!(a, b);
    }

    #[test]
    fn hash_ignores_non_xml_and_changes_on_edit_or_add() {
        let temp = TempDir::new().expect("tempdir");
        let dir = temp.path();
        fs::create_dir_all(dir.join("nested")).expect("mkdir");
        fs::write(dir.join("nested").join("defs.xml"), "<Defs/>").expect("write defs");
        fs::write(dir.join("notes.txt"), "ignore me").expect("write txt");

        let first = hash_mod_xml_inputs(dir).expect("hash");
        assert_eq!(first.xml_file_count, 1);

        fs::write(dir.join("nested").join("defs.xml"), "<Defs><A/></Defs>").expect("edit");
        let second = hash_mod_xml_inputs(dir).expect("hash");
        assert_ne!(first.hash_hex, second.hash_hex);

        fs::write(dir.join("new.xml"), "<Defs><B/></Defs>").expect("add xml");
        let third = hash_mod_xml_inputs(dir).expect("hash");
        assert_eq!(third.xml_file_count, 2);
        assert_ne!(second.hash_hex, third.hash_hex);
    }
}
