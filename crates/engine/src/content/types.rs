use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ContentPlanRequest {
    pub enabled_mods: Vec<String>,
    pub compiler_version: String,
    pub game_version: String,
}

impl Default for ContentPlanRequest {
    fn default() -> Self {
        Self {
            enabled_mods: Vec::new(),
            compiler_version: "dev".to_string(),
            game_version: "dev".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileAction {
    UseCache,
    Compile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileReason {
    CacheValid,
    ManifestMissing,
    ManifestUnreadable,
    PackMissing,
    VersionMismatch,
    EnabledModsHashMismatch,
    InputHashMismatch,
    ModLoadIndexMismatch,
    ModIdMismatch,
    PackFormatMismatch,
}

#[derive(Debug, Clone)]
pub struct ModCompileDecision {
    pub mod_id: String,
    pub mod_load_index: u32,
    pub source_dir: PathBuf,
    pub xml_file_count: usize,
    pub input_hash_sha256_hex: String,
    pub pack_path: PathBuf,
    pub manifest_path: PathBuf,
    pub action: CompileAction,
    pub reason: CompileReason,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ContentStatusSummary {
    pub total_mods: usize,
    pub compile_count: usize,
    pub cache_hit_count: usize,
}

impl ContentStatusSummary {
    pub fn status_label(&self) -> &'static str {
        if self.compile_count > 0 {
            "compiling"
        } else {
            "loaded"
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompilePlan {
    pub decisions: Vec<ModCompileDecision>,
    pub enabled_mods_hash_sha256_hex: String,
    pub summary: ContentStatusSummary,
}

impl CompilePlan {
    pub fn render_human_readable(&self) -> String {
        let mut output = format!(
            "total_mods={} compile={} cache_hits={} status={} enabled_mods_hash={}",
            self.summary.total_mods,
            self.summary.compile_count,
            self.summary.cache_hit_count,
            self.summary.status_label(),
            self.enabled_mods_hash_sha256_hex
        );
        for decision in &self.decisions {
            output.push('\n');
            output.push_str(&format!(
                "mod={} index={} action={:?} reason={:?} xml_files={} input_hash={} pack={} manifest={}",
                decision.mod_id,
                decision.mod_load_index,
                decision.action,
                decision.reason,
                decision.xml_file_count,
                decision.input_hash_sha256_hex,
                decision.pack_path.display(),
                decision.manifest_path.display()
            ));
        }
        output
    }
}

#[derive(Debug, Error)]
pub enum ContentPlanError {
    #[error("enabled mod id cannot be empty")]
    EmptyEnabledMod,
    #[error("duplicate enabled mod id in request: {mod_id}")]
    DuplicateEnabledMod { mod_id: String },
    #[error("enabled mod does not exist on disk: {mod_id} at {expected_dir}")]
    EnabledModMissing {
        mod_id: String,
        expected_dir: PathBuf,
    },
    #[error("failed to read directory {path}: {source}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read directory entry in {path}: {source}")]
    ReadDirEntry {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to create cache layout at {path}: {source}")]
    CreateCacheLayout {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
