use std::collections::BTreeMap;

use thiserror::Error;
use tracing::{info, warn};

use crate::AppPaths;

use super::compiler::{
    compile_mod_entity_defs, def_database_from_merged, CompiledEntityDef, ContentCompileError,
};
use super::database::DefDatabase;
use super::manifest::{
    read_manifest, write_manifest_atomic, ManifestReadState, ManifestV1,
    CONTENT_PACK_FORMAT_VERSION,
};
use super::pack::{
    read_content_pack_v1, write_content_pack_v1, ContentPackError, ContentPackMeta, PackedEntityDef,
};
use super::planner::build_compile_plan;
use super::types::{CompileAction, ContentPlanError, ContentPlanRequest, ModCompileDecision};

#[derive(Debug, Error)]
pub enum ContentPipelineError {
    #[error(transparent)]
    Plan(#[from] ContentPlanError),
    #[error(transparent)]
    Compile(#[from] ContentCompileError),
    #[error(transparent)]
    Pack(#[from] ContentPackError),
}

pub fn build_or_load_def_database(
    app_paths: &AppPaths,
    request: &ContentPlanRequest,
) -> Result<DefDatabase, ContentPipelineError> {
    let compile_plan = build_compile_plan(app_paths, request)?;
    for decision in &compile_plan.decisions {
        info!(
            mod_id = %decision.mod_id,
            mod_load_index = decision.mod_load_index,
            action = ?decision.action,
            reason = ?decision.reason,
            xml_file_count = decision.xml_file_count,
            input_hash = %decision.input_hash_sha256_hex,
            pack_path = %decision.pack_path.display(),
            manifest_path = %decision.manifest_path.display(),
            "content_compile_plan_decision"
        );
    }
    let mut merged = BTreeMap::<String, CompiledEntityDef>::new();

    for decision in &compile_plan.decisions {
        let defs = match decision.action {
            CompileAction::Compile => compile_and_write_mod(
                decision,
                request,
                &compile_plan.enabled_mods_hash_sha256_hex,
            )?,
            CompileAction::UseCache => match try_load_cached_mod(
                decision,
                request,
                &compile_plan.enabled_mods_hash_sha256_hex,
            ) {
                Ok(defs) => defs,
                Err(reason) => {
                    warn!(
                        mod_id = %decision.mod_id,
                        mod_load_index = decision.mod_load_index,
                        reason = %reason,
                        "content_cache_invalid_rebuilding_mod"
                    );
                    compile_and_write_mod(
                        decision,
                        request,
                        &compile_plan.enabled_mods_hash_sha256_hex,
                    )?
                }
            },
        };

        for def in defs {
            merged.insert(def.def_name.clone(), def);
        }
    }

    let summary = compile_plan.summary;
    info!(
        total_mods = summary.total_mods,
        compile_count = summary.compile_count,
        cache_hit_count = summary.cache_hit_count,
        content_status = summary.status_label(),
        enabled_mods_hash = %compile_plan.enabled_mods_hash_sha256_hex,
        "content_pipeline_summary"
    );

    Ok(def_database_from_merged(merged))
}

fn compile_and_write_mod(
    decision: &ModCompileDecision,
    request: &ContentPlanRequest,
    enabled_mods_hash_sha256_hex: &str,
) -> Result<Vec<CompiledEntityDef>, ContentPipelineError> {
    let defs = compile_mod_entity_defs(&decision.source_dir, &decision.mod_id)?;
    let manifest = expected_manifest(decision, request, enabled_mods_hash_sha256_hex);
    let meta = manifest_to_meta(&manifest);
    write_content_pack_v1(&decision.pack_path, &meta, &defs)?;
    write_manifest_atomic(&decision.manifest_path, &manifest)?;
    Ok(defs)
}

fn try_load_cached_mod(
    decision: &ModCompileDecision,
    request: &ContentPlanRequest,
    enabled_mods_hash_sha256_hex: &str,
) -> Result<Vec<CompiledEntityDef>, String> {
    let expected_manifest = expected_manifest(decision, request, enabled_mods_hash_sha256_hex);
    let manifest = match read_manifest(&decision.manifest_path) {
        Ok(ManifestReadState::Present(manifest)) => manifest,
        Ok(ManifestReadState::Missing) => return Err("manifest missing".to_string()),
        Ok(ManifestReadState::Unreadable) => return Err("manifest unreadable".to_string()),
        Err(error) => return Err(format!("failed to read manifest: {error}")),
    };

    validate_manifest_matches_expected(&manifest, &expected_manifest)?;

    let pack = read_content_pack_v1(&decision.pack_path)
        .map_err(|error| format!("failed to read pack: {error}"))?;
    validate_pack_meta_matches_manifest(&pack.meta, &manifest)?;

    Ok(pack.records.into_iter().map(compiled_from_packed).collect())
}

fn expected_manifest(
    decision: &ModCompileDecision,
    request: &ContentPlanRequest,
    enabled_mods_hash_sha256_hex: &str,
) -> ManifestV1 {
    ManifestV1 {
        pack_format_version: CONTENT_PACK_FORMAT_VERSION,
        compiler_version: request.compiler_version.clone(),
        game_version: request.game_version.clone(),
        mod_id: decision.mod_id.clone(),
        mod_load_index: decision.mod_load_index,
        enabled_mods_hash_sha256_hex: enabled_mods_hash_sha256_hex.to_string(),
        input_hash_sha256_hex: decision.input_hash_sha256_hex.clone(),
    }
}

fn manifest_to_meta(manifest: &ManifestV1) -> ContentPackMeta {
    ContentPackMeta {
        pack_format_version: manifest.pack_format_version,
        compiler_version: manifest.compiler_version.clone(),
        game_version: manifest.game_version.clone(),
        mod_id: manifest.mod_id.clone(),
        mod_load_index: manifest.mod_load_index,
        enabled_mods_hash_sha256_hex: manifest.enabled_mods_hash_sha256_hex.clone(),
        input_hash_sha256_hex: manifest.input_hash_sha256_hex.clone(),
    }
}

fn validate_manifest_matches_expected(
    manifest: &ManifestV1,
    expected: &ManifestV1,
) -> Result<(), String> {
    if manifest.pack_format_version != expected.pack_format_version {
        return Err("manifest pack_format_version mismatch".to_string());
    }
    if manifest.compiler_version != expected.compiler_version {
        return Err("manifest compiler_version mismatch".to_string());
    }
    if manifest.game_version != expected.game_version {
        return Err("manifest game_version mismatch".to_string());
    }
    if manifest.mod_id != expected.mod_id {
        return Err("manifest mod_id mismatch".to_string());
    }
    if manifest.mod_load_index != expected.mod_load_index {
        return Err("manifest mod_load_index mismatch".to_string());
    }
    if manifest.enabled_mods_hash_sha256_hex != expected.enabled_mods_hash_sha256_hex {
        return Err("manifest enabled_mods_hash mismatch".to_string());
    }
    if manifest.input_hash_sha256_hex != expected.input_hash_sha256_hex {
        return Err("manifest input_hash mismatch".to_string());
    }
    Ok(())
}

fn validate_pack_meta_matches_manifest(
    pack_meta: &ContentPackMeta,
    manifest: &ManifestV1,
) -> Result<(), String> {
    if pack_meta.pack_format_version != manifest.pack_format_version {
        return Err("pack header pack_format_version mismatch vs manifest".to_string());
    }
    if pack_meta.compiler_version != manifest.compiler_version {
        return Err("pack header compiler_version mismatch vs manifest".to_string());
    }
    if pack_meta.game_version != manifest.game_version {
        return Err("pack header game_version mismatch vs manifest".to_string());
    }
    if pack_meta.mod_id != manifest.mod_id {
        return Err("pack header mod_id mismatch vs manifest".to_string());
    }
    if pack_meta.mod_load_index != manifest.mod_load_index {
        return Err("pack header mod_load_index mismatch vs manifest".to_string());
    }
    if pack_meta.enabled_mods_hash_sha256_hex != manifest.enabled_mods_hash_sha256_hex {
        return Err("pack header enabled_mods_hash mismatch vs manifest".to_string());
    }
    if pack_meta.input_hash_sha256_hex != manifest.input_hash_sha256_hex {
        return Err("pack header input_hash mismatch vs manifest".to_string());
    }
    Ok(())
}

fn compiled_from_packed(packed: PackedEntityDef) -> CompiledEntityDef {
    CompiledEntityDef {
        def_name: packed.def_name,
        label: packed.label,
        renderable: packed.renderable,
        move_speed: packed.move_speed,
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

    fn request() -> ContentPlanRequest {
        ContentPlanRequest {
            enabled_mods: vec!["moda".to_string()],
            compiler_version: "test-compiler".to_string(),
            game_version: "test-game".to_string(),
        }
    }

    fn seed_base_and_mod(app: &AppPaths) {
        fs::create_dir_all(app.mods_dir.join("moda")).expect("mkdir moda");
        write_xml(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Base</label><renderable>Placeholder</renderable><moveSpeed>5.0</moveSpeed></EntityDef></Defs>"#,
        );
        write_xml(
            &app.mods_dir.join("moda").join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Moda</label><renderable>Placeholder</renderable><moveSpeed>7.0</moveSpeed></EntityDef></Defs>"#,
        );
    }

    #[test]
    fn first_run_builds_cache_and_second_run_reads_it() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        seed_base_and_mod(&app);

        let req = request();
        let first = build_or_load_def_database(&app, &req).expect("first");
        assert!(first.entity_def_id_by_name("proto.player").is_some());

        let second = build_or_load_def_database(&app, &req).expect("second");
        let player_id = second
            .entity_def_id_by_name("proto.player")
            .expect("player");
        let player = second.entity_def(player_id).expect("player def");
        assert_eq!(player.label, "Moda");
    }

    #[test]
    fn edit_in_one_mod_rebuilds_and_updates_only_that_mod() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        seed_base_and_mod(&app);

        let req = request();
        let _ = build_or_load_def_database(&app, &req).expect("build");
        write_xml(
            &app.mods_dir.join("moda").join("defs.xml"),
            r#"<Defs><EntityDef><defName>proto.player</defName><label>Moda2</label><renderable>Placeholder</renderable><moveSpeed>9.0</moveSpeed></EntityDef></Defs>"#,
        );
        let db = build_or_load_def_database(&app, &req).expect("reload");
        let id = db.entity_def_id_by_name("proto.player").expect("id");
        let player = db.entity_def(id).expect("player");
        assert_eq!(player.label, "Moda2");
    }

    #[test]
    fn corrupt_pack_rebuilds_from_xml() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        seed_base_and_mod(&app);
        let req = request();
        let _ = build_or_load_def_database(&app, &req).expect("build");

        let moda_pack = app.cache_dir.join("content_packs").join("moda.pack");
        fs::write(&moda_pack, b"not a valid pack").expect("corrupt pack");

        let db = build_or_load_def_database(&app, &req).expect("rebuild");
        let id = db.entity_def_id_by_name("proto.player").expect("id");
        assert_eq!(db.entity_def(id).expect("def").label, "Moda");
    }

    #[test]
    fn header_manifest_mismatch_rebuilds() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        seed_base_and_mod(&app);
        let req = request();
        let _ = build_or_load_def_database(&app, &req).expect("build");

        let base_pack = app.cache_dir.join("content_packs").join("base.pack");
        let mut bytes = fs::read(&base_pack).expect("read base pack");
        bytes[14] = 1; // corrupt mod_load_index in header (base should be 0)
        fs::write(&base_pack, &bytes).expect("write corrupt base pack");

        let _ = build_or_load_def_database(&app, &req).expect("rebuild");
        let loaded = read_content_pack_v1(&base_pack).expect("read repaired");
        assert_eq!(loaded.meta.mod_load_index, 0);
    }

    #[test]
    fn compile_failure_is_fatal() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("moda")).expect("mkdir moda");
        write_xml(
            &app.base_content_dir.join("defs.xml"),
            r#"<Defs><EntityDef><label>Missing defName</label><renderable>Placeholder</renderable></EntityDef></Defs>"#,
        );
        write_xml(&app.mods_dir.join("moda").join("defs.xml"), "<Defs/>");

        let error = build_or_load_def_database(&app, &request()).expect_err("error");
        assert!(matches!(error, ContentPipelineError::Compile(_)));
    }
}
