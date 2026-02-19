use std::fs;

use crate::AppPaths;

use super::discovery::discover_mod_sources;
use super::hashing::{hash_enabled_mods_list, hash_mod_xml_inputs};
use super::manifest::{
    content_pack_cache_dir, manifest_path, pack_path, read_manifest, ManifestReadState,
    CONTENT_PACK_FORMAT_VERSION,
};
use super::types::{
    CompileAction, CompilePlan, CompileReason, ContentPlanError, ContentPlanRequest,
    ContentStatusSummary, ModCompileDecision,
};

pub fn build_compile_plan(
    app_paths: &AppPaths,
    request: &ContentPlanRequest,
) -> Result<CompilePlan, ContentPlanError> {
    let mod_sources = discover_mod_sources(app_paths, request)?;
    let mod_ids = mod_sources
        .iter()
        .map(|source| source.mod_id.clone())
        .collect::<Vec<_>>();
    let enabled_mods_hash_sha256_hex = hash_enabled_mods_list(&mod_ids);

    let pack_cache_dir = content_pack_cache_dir(&app_paths.cache_dir);
    fs::create_dir_all(&pack_cache_dir).map_err(|source| ContentPlanError::CreateCacheLayout {
        path: pack_cache_dir.clone(),
        source,
    })?;

    let mut decisions = Vec::<ModCompileDecision>::new();
    for source in mod_sources {
        let input = hash_mod_xml_inputs(&source.source_dir)?;
        let pack_path = pack_path(&app_paths.cache_dir, &source.mod_id);
        let manifest_path = manifest_path(&app_paths.cache_dir, &source.mod_id);
        let (action, reason) = evaluate_cache_validity(
            &manifest_path,
            &pack_path,
            request,
            &source.mod_id,
            source.mod_load_index,
            &enabled_mods_hash_sha256_hex,
            &input.hash_hex,
        )?;

        decisions.push(ModCompileDecision {
            mod_id: source.mod_id,
            mod_load_index: source.mod_load_index,
            source_dir: source.source_dir,
            xml_file_count: input.xml_file_count,
            input_hash_sha256_hex: input.hash_hex,
            pack_path,
            manifest_path,
            action,
            reason,
        });
    }

    let summary = summarize(&decisions);
    Ok(CompilePlan {
        decisions,
        enabled_mods_hash_sha256_hex,
        summary,
    })
}

#[allow(clippy::too_many_arguments)]
fn evaluate_cache_validity(
    manifest_path: &std::path::Path,
    pack_path: &std::path::Path,
    request: &ContentPlanRequest,
    mod_id: &str,
    mod_load_index: u32,
    enabled_mods_hash_sha256_hex: &str,
    input_hash_sha256_hex: &str,
) -> Result<(CompileAction, CompileReason), ContentPlanError> {
    let manifest = read_manifest(manifest_path)?;
    match manifest {
        ManifestReadState::Missing => {
            return Ok((CompileAction::Compile, CompileReason::ManifestMissing))
        }
        ManifestReadState::Unreadable => {
            return Ok((CompileAction::Compile, CompileReason::ManifestUnreadable))
        }
        ManifestReadState::Present(value) => {
            if value.pack_format_version != CONTENT_PACK_FORMAT_VERSION {
                return Ok((CompileAction::Compile, CompileReason::PackFormatMismatch));
            }
            if value.compiler_version != request.compiler_version
                || value.game_version != request.game_version
            {
                return Ok((CompileAction::Compile, CompileReason::VersionMismatch));
            }
            if value.mod_id != mod_id {
                return Ok((CompileAction::Compile, CompileReason::ModIdMismatch));
            }
            if value.mod_load_index != mod_load_index {
                return Ok((CompileAction::Compile, CompileReason::ModLoadIndexMismatch));
            }
            if value.enabled_mods_hash_sha256_hex != enabled_mods_hash_sha256_hex {
                return Ok((
                    CompileAction::Compile,
                    CompileReason::EnabledModsHashMismatch,
                ));
            }
            if value.input_hash_sha256_hex != input_hash_sha256_hex {
                return Ok((CompileAction::Compile, CompileReason::InputHashMismatch));
            }
        }
    }

    if !pack_path.is_file() {
        return Ok((CompileAction::Compile, CompileReason::PackMissing));
    }
    Ok((CompileAction::UseCache, CompileReason::CacheValid))
}

fn summarize(decisions: &[ModCompileDecision]) -> ContentStatusSummary {
    let compile_count = decisions
        .iter()
        .filter(|decision| decision.action == CompileAction::Compile)
        .count();
    let cache_hit_count = decisions
        .iter()
        .filter(|decision| decision.action == CompileAction::UseCache)
        .count();
    ContentStatusSummary {
        total_mods: decisions.len(),
        compile_count,
        cache_hit_count,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn setup_app_paths(root: &std::path::Path) -> AppPaths {
        let base = root.join("assets").join("base");
        let mods = root.join("mods");
        let cache = root.join("cache");
        fs::create_dir_all(&base).expect("base");
        fs::create_dir_all(&mods).expect("mods");
        fs::create_dir_all(&cache).expect("cache");
        AppPaths {
            root: root.to_path_buf(),
            base_content_dir: base,
            mods_dir: mods,
            cache_dir: cache,
        }
    }

    fn write_xml(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, content).expect("write xml");
    }

    fn write_manifest(
        path: &std::path::Path,
        compiler_version: &str,
        game_version: &str,
        mod_id: &str,
        mod_load_index: u32,
        enabled_mods_hash: &str,
        input_hash: &str,
    ) {
        let body = format!(
            "{{\"pack_format_version\":1,\"compiler_version\":\"{compiler_version}\",\"game_version\":\"{game_version}\",\"mod_id\":\"{mod_id}\",\"mod_load_index\":{mod_load_index},\"enabled_mods_hash_sha256_hex\":\"{enabled_mods_hash}\",\"input_hash_sha256_hex\":\"{input_hash}\"}}"
        );
        fs::write(path, body).expect("write manifest");
    }

    #[test]
    fn missing_manifest_forces_compile() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_xml(&app.base_content_dir.join("defs.xml"), "<Defs/>");
        fs::create_dir_all(app.mods_dir.join("a")).expect("mkdir");
        write_xml(&app.mods_dir.join("a").join("defs.xml"), "<Defs/>");

        let plan = build_compile_plan(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["a".to_string()],
                compiler_version: "1".to_string(),
                game_version: "1".to_string(),
            },
        )
        .expect("plan");
        assert!(plan
            .decisions
            .iter()
            .all(|decision| decision.reason == CompileReason::ManifestMissing));
    }

    #[test]
    fn exact_manifest_match_uses_cache() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_xml(&app.base_content_dir.join("defs.xml"), "<Defs/>");
        fs::create_dir_all(app.mods_dir.join("a")).expect("mkdir");
        write_xml(&app.mods_dir.join("a").join("defs.xml"), "<Defs/>");
        let request = ContentPlanRequest {
            enabled_mods: vec!["a".to_string()],
            compiler_version: "1".to_string(),
            game_version: "1".to_string(),
        };
        let initial_plan = build_compile_plan(&app, &request).expect("plan");
        for decision in &initial_plan.decisions {
            write_manifest(
                &decision.manifest_path,
                &request.compiler_version,
                &request.game_version,
                &decision.mod_id,
                decision.mod_load_index,
                &initial_plan.enabled_mods_hash_sha256_hex,
                &decision.input_hash_sha256_hex,
            );
            fs::write(&decision.pack_path, b"placeholder").expect("pack");
        }

        let second_plan = build_compile_plan(&app, &request).expect("plan");
        assert!(second_plan
            .decisions
            .iter()
            .all(|decision| decision.action == CompileAction::UseCache));
    }

    #[test]
    fn version_mismatch_forces_compile() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_xml(&app.base_content_dir.join("defs.xml"), "<Defs/>");
        let request = ContentPlanRequest {
            enabled_mods: vec![],
            compiler_version: "1".to_string(),
            game_version: "1".to_string(),
        };
        let plan = build_compile_plan(&app, &request).expect("plan");
        let base = &plan.decisions[0];
        write_manifest(
            &base.manifest_path,
            "2",
            &request.game_version,
            &base.mod_id,
            base.mod_load_index,
            &plan.enabled_mods_hash_sha256_hex,
            &base.input_hash_sha256_hex,
        );
        fs::write(&base.pack_path, b"placeholder").expect("pack");

        let next = build_compile_plan(&app, &request).expect("plan");
        assert_eq!(next.decisions[0].reason, CompileReason::VersionMismatch);
        assert_eq!(next.decisions[0].action, CompileAction::Compile);
    }

    #[test]
    fn mod_load_index_mismatch_forces_compile() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        write_xml(&app.base_content_dir.join("defs.xml"), "<Defs/>");
        let request = ContentPlanRequest {
            enabled_mods: vec![],
            compiler_version: "1".to_string(),
            game_version: "1".to_string(),
        };
        let plan = build_compile_plan(&app, &request).expect("plan");
        let base = &plan.decisions[0];
        write_manifest(
            &base.manifest_path,
            &request.compiler_version,
            &request.game_version,
            &base.mod_id,
            999,
            &plan.enabled_mods_hash_sha256_hex,
            &base.input_hash_sha256_hex,
        );
        fs::write(&base.pack_path, b"placeholder").expect("pack");

        let next = build_compile_plan(&app, &request).expect("plan");
        assert_eq!(
            next.decisions[0].reason,
            CompileReason::ModLoadIndexMismatch
        );
    }

    #[test]
    fn one_mod_change_invalidates_only_that_mod() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("a")).expect("mkdir");
        fs::create_dir_all(app.mods_dir.join("b")).expect("mkdir");
        write_xml(&app.base_content_dir.join("defs.xml"), "<Defs/>");
        write_xml(
            &app.mods_dir.join("a").join("defs.xml"),
            "<Defs><A/></Defs>",
        );
        write_xml(
            &app.mods_dir.join("b").join("defs.xml"),
            "<Defs><B/></Defs>",
        );
        let request = ContentPlanRequest {
            enabled_mods: vec!["a".to_string(), "b".to_string()],
            compiler_version: "1".to_string(),
            game_version: "1".to_string(),
        };
        let initial = build_compile_plan(&app, &request).expect("plan");
        for decision in &initial.decisions {
            write_manifest(
                &decision.manifest_path,
                &request.compiler_version,
                &request.game_version,
                &decision.mod_id,
                decision.mod_load_index,
                &initial.enabled_mods_hash_sha256_hex,
                &decision.input_hash_sha256_hex,
            );
            fs::write(&decision.pack_path, b"placeholder").expect("pack");
        }

        write_xml(
            &app.mods_dir.join("a").join("defs.xml"),
            "<Defs><A2/></Defs>",
        );
        let next = build_compile_plan(&app, &request).expect("plan");
        let a = next
            .decisions
            .iter()
            .find(|decision| decision.mod_id == "a")
            .expect("mod a");
        let b = next
            .decisions
            .iter()
            .find(|decision| decision.mod_id == "b")
            .expect("mod b");
        assert_eq!(a.action, CompileAction::Compile);
        assert_eq!(a.reason, CompileReason::InputHashMismatch);
        assert_eq!(b.action, CompileAction::UseCache);
    }
}
