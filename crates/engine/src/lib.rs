use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

pub mod app;
pub mod content;
mod sprite_keys;

pub use app::{
    run_app, run_app_with_metrics, screen_to_world_px, world_to_screen_px, AppError, Camera2D,
    DebugInfoSnapshot, DebugJobState, DebugMarker, DebugMarkerKind, Entity, EntityId, InputAction,
    InputSnapshot, Interactable, InteractableKind, JobState, LoopConfig, LoopMetricsSnapshot,
    MetricsHandle, RenderableDesc, RenderableKind, Renderer, Scene, SceneCommand, SceneKey,
    SceneVisualState, SceneWorld, Tilemap, TilemapError, Transform, Vec2, Viewport,
    PIXELS_PER_WORLD, PLACEHOLDER_HALF_SIZE_PX, SLOW_FRAME_ENV_VAR,
};
pub use content::{
    build_compile_plan, build_or_load_def_database, compile_def_database, CompileAction,
    CompilePlan, CompileReason, ContentCompileError, ContentErrorCode, ContentPipelineError,
    ContentPlanError, ContentPlanRequest, ContentStatusSummary, DefDatabase, EntityArchetype,
    EntityDefId, ModCompileDecision, SourceLocation,
};

pub const ROOT_ENV_VAR: &str = "PROTOGE_ROOT";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub base_content_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to read environment variable {var}: {source}")]
    EnvVar {
        var: &'static str,
        #[source]
        source: env::VarError,
    },
    #[error("failed to resolve current executable path: {0}")]
    CurrentExe(#[source] std::io::Error),
    #[error("current executable path has no parent directory: {0}")]
    ExeHasNoParent(PathBuf),
    #[error("failed to create cache directory at {path}: {source}")]
    CreateCacheDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "PROTOGE_ROOT is set but does not point to a valid project root: {path}\n\
A valid root must contain Cargo.toml and either crates/ or assets/."
    )]
    InvalidEnvRoot { path: PathBuf },
    #[error(
        "Could not detect project root by walking upward from executable directory: {start_dir}\n\
Expected a directory containing Cargo.toml and either crates/ or assets/.\n\
Set {env_var} explicitly, for example:\n\
PowerShell: $env:{env_var}=\"C:\\path\\to\\Prototype Game Engine\"\n\
Bash/zsh: export {env_var}=\"/path/to/Prototype Game Engine\""
    )]
    RootNotFound {
        start_dir: PathBuf,
        env_var: &'static str,
    },
}

pub fn resolve_app_paths() -> Result<AppPaths, StartupError> {
    let root = resolve_root()?;
    let base_content_dir = root.join("assets").join("base");
    let mods_dir = root.join("mods");
    let cache_dir = root.join("cache");

    fs::create_dir_all(&cache_dir).map_err(|source| StartupError::CreateCacheDir {
        path: cache_dir.clone(),
        source,
    })?;

    Ok(AppPaths {
        root,
        base_content_dir,
        mods_dir,
        cache_dir,
    })
}

fn resolve_root() -> Result<PathBuf, StartupError> {
    match env::var(ROOT_ENV_VAR) {
        Ok(value) => {
            let raw = PathBuf::from(value);
            let normalized = normalize_path(&raw);
            if is_repo_marker(&normalized) {
                Ok(normalized)
            } else {
                Err(StartupError::InvalidEnvRoot { path: normalized })
            }
        }
        Err(env::VarError::NotPresent) => {
            let exe = env::current_exe().map_err(StartupError::CurrentExe)?;
            let exe_dir = exe
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| StartupError::ExeHasNoParent(exe.clone()))?;

            for candidate in exe_dir.ancestors() {
                if is_repo_marker(candidate) {
                    return Ok(normalize_path(candidate));
                }
            }

            Err(StartupError::RootNotFound {
                start_dir: normalize_path(&exe_dir),
                env_var: ROOT_ENV_VAR,
            })
        }
        Err(source) => Err(StartupError::EnvVar {
            var: ROOT_ENV_VAR,
            source,
        }),
    }
}

fn is_repo_marker(path: &Path) -> bool {
    let cargo_toml = path.join("Cargo.toml").is_file();
    let has_crates = path.join("crates").is_dir();
    let has_assets = path.join("assets").is_dir();

    cargo_toml && (has_crates || has_assets)
}

fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_marker_requires_cargo_toml() {
        let cwd = env::current_dir().expect("cwd");
        assert!(!is_repo_marker(&cwd.join("definitely_not_a_marker")));
    }
}
