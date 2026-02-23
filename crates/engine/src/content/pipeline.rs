use thiserror::Error;
use tracing::{info, warn};

use crate::AppPaths;

use super::compiler::{
    compile_mod_entity_defs, def_database_from_compiled_defs, CompiledEntityDef,
    ContentCompileError,
};
use super::database::DefDatabase;
use super::manifest::{
    read_manifest, write_manifest_atomic, ManifestReadState, ManifestV1,
    CONTENT_PACK_FORMAT_VERSION,
};
use super::pack::{
    compiled_from_packed, read_content_pack_v1, write_content_pack_v1, ContentPackError,
    ContentPackMeta,
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
    let mut merged = Vec::<CompiledEntityDef>::new();

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
                Ok(defs) => {
                    info!(
                        mod_id = %decision.mod_id,
                        mod_load_index = decision.mod_load_index,
                        pack_path = %decision.pack_path.display(),
                        manifest_path = %decision.manifest_path.display(),
                        input_hash = %decision.input_hash_sha256_hex,
                        enabled_mods_hash = %compile_plan.enabled_mods_hash_sha256_hex,
                        "content_cache_hit"
                    );
                    defs
                }
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

        merged.extend(defs);
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

    Ok(def_database_from_compiled_defs(merged)?)
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

    Ok(pack
        .records
        .into_iter()
        .map(|packed| compiled_from_packed(packed, &decision.mod_id, &decision.pack_path))
        .collect())
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

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

    fn fixture_root(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("fixtures")
            .join("content_pipeline_v1")
            .join(name)
    }

    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).expect("mkdir dst");
        let entries = fs::read_dir(src).expect("read src");
        for entry in entries {
            let entry = entry.expect("entry");
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).expect("copy");
            }
        }
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
        bytes[12] = 1; // corrupt mod_load_index in header (base should be 0)
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

    #[test]
    fn fixture_invalid_gameplay_field_fails_with_structured_context() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        fs::create_dir_all(app.mods_dir.join("badgameplay")).expect("mkdir badgameplay");
        copy_dir_recursive(
            &fixture_root("fail_09_invalid_gameplay_field").join("badgameplay"),
            &app.mods_dir.join("badgameplay"),
        );

        let error = build_or_load_def_database(
            &app,
            &ContentPlanRequest {
                enabled_mods: vec!["badgameplay".to_string()],
                compiler_version: "dev".to_string(),
                game_version: "dev".to_string(),
            },
        )
        .expect_err("error");

        let ContentPipelineError::Compile(err) = error else {
            panic!("expected compile error");
        };
        assert_eq!(
            err.code,
            super::super::compiler::ContentErrorCode::InvalidValue
        );
        assert_eq!(err.mod_id, "badgameplay");
        assert_eq!(err.def_name.as_deref(), Some("proto.badgameplay"));
        assert_eq!(err.field_name.as_deref(), Some("attack_cooldown_seconds"));
        assert!(err.message.contains("attack_cooldown_seconds"));
    }

    #[test]
    fn base_defs_load_proto_npc_chaser_with_expected_tuning_fields() {
        let temp = TempDir::new().expect("temp");
        let app = setup_app_paths(temp.path());
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let source_defs = workspace_root.join("assets").join("base").join("defs.xml");
        fs::copy(&source_defs, app.base_content_dir.join("defs.xml")).expect("copy defs");

        let db = build_or_load_def_database(&app, &ContentPlanRequest::default()).expect("load");
        let chaser_id = db
            .entity_def_id_by_name("proto.npc_chaser")
            .expect("proto.npc_chaser");
        let chaser = db.entity_def(chaser_id).expect("chaser def");
        assert_eq!(chaser.health_max, Some(200));
        assert_eq!(chaser.base_damage, Some(40));
        assert_eq!(chaser.aggro_radius, Some(10.0));
        assert_eq!(chaser.attack_range, Some(1.2));
        assert_eq!(chaser.attack_cooldown_seconds, Some(0.6));
    }
}
