use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::AppPaths;

use super::types::{ContentPlanError, ContentPlanRequest};

#[derive(Debug, Clone)]
pub(crate) struct ModSource {
    pub mod_id: String,
    pub mod_load_index: u32,
    pub source_dir: PathBuf,
}

pub(crate) fn discover_mod_sources(
    app_paths: &AppPaths,
    request: &ContentPlanRequest,
) -> Result<Vec<ModSource>, ContentPlanError> {
    let mut seen = HashSet::<String>::new();
    let mut sources = vec![ModSource {
        mod_id: "base".to_string(),
        mod_load_index: 0,
        source_dir: app_paths.base_content_dir.clone(),
    }];

    for (idx, mod_id) in request.enabled_mods.iter().enumerate() {
        let trimmed = mod_id.trim();
        if trimmed.is_empty() {
            return Err(ContentPlanError::EmptyEnabledMod);
        }
        if !seen.insert(trimmed.to_string()) {
            return Err(ContentPlanError::DuplicateEnabledMod {
                mod_id: trimmed.to_string(),
            });
        }
        let mod_dir = app_paths.mods_dir.join(trimmed);
        ensure_dir_exists(trimmed, &mod_dir)?;
        sources.push(ModSource {
            mod_id: trimmed.to_string(),
            mod_load_index: (idx + 1) as u32,
            source_dir: mod_dir,
        });
    }

    Ok(sources)
}

fn ensure_dir_exists(mod_id: &str, path: &Path) -> Result<(), ContentPlanError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(ContentPlanError::EnabledModMissing {
            mod_id: mod_id.to_string(),
            expected_dir: path.to_path_buf(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::AppPaths;

    #[test]
    fn base_is_first_then_enabled_order() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        let base = root.join("assets").join("base");
        let mods = root.join("mods");
        fs::create_dir_all(base).expect("create base");
        fs::create_dir_all(mods.join("b")).expect("create mod b");
        fs::create_dir_all(mods.join("a")).expect("create mod a");
        let app_paths = AppPaths {
            root: root.to_path_buf(),
            base_content_dir: root.join("assets").join("base"),
            mods_dir: root.join("mods"),
            cache_dir: root.join("cache"),
        };
        let request = ContentPlanRequest {
            enabled_mods: vec!["b".to_string(), "a".to_string()],
            compiler_version: "1".to_string(),
            game_version: "1".to_string(),
        };

        let sources = discover_mod_sources(&app_paths, &request).expect("discover");
        assert_eq!(sources[0].mod_id, "base");
        assert_eq!(sources[1].mod_id, "b");
        assert_eq!(sources[2].mod_id, "a");
        assert_eq!(sources[0].mod_load_index, 0);
        assert_eq!(sources[1].mod_load_index, 1);
        assert_eq!(sources[2].mod_load_index, 2);
    }
}
