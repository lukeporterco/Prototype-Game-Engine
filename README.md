# Prototype Game Engine

Prototype-first Rust engine bootstrap for Proto GE.

## Reference

- `docs/PROTOGE_INFRASTRUCTURE_REFERENCE.md`

## Run

```powershell
cargo run
```

This opens a window, runs the main loop, logs periodic loop metrics, and exits cleanly when you close the window or press `Esc`.

## Root Resolution

At startup the app resolves `root`, `assets/base`, `mods`, and `cache` using this order:

1. `PROTOGE_ROOT` environment variable (if set)
2. Otherwise, walk upward from the executable directory and pick the first directory that contains:
   - `Cargo.toml`
   - and either `crates/` or `assets/`

If no matching root is found, startup fails fast with instructions.

## Loop and Metrics

- Fixed timestep simulation runs at 60 TPS by default.
- Rendering runs separately from simulation updates.
- Structured loop metrics are logged once per second:
  - `fps`
  - `tps`
  - `frame_time_ms`
- Simulation backlog is clamped to prevent runaway spirals on slow frames.
- Quit paths:
  - Window close button
  - `Esc` key

## Input and Movement (Ticket 3)

- Engine maps keyboard to actions: `MoveUp/Down/Left/Right`, `Quit`.
- Supported movement keys: `WASD` and arrow keys.
- Game scene consumes engine `InputSnapshot` actions with no `winit` dependency.
- Player movement runs in fixed-timestep update (`5.0` units/second), so distance is stable across FPS changes.
- Game exposes optional debug title text (`Scene::debug_title`), and engine applies it to the window title.

## Rendering and Camera (Ticket 4)

- Engine renders placeholder entity shapes to pixels using a world-to-screen transform.
- Renderer reads only `SceneWorld` data:
  - entities (`SceneWorld.entities()`)
  - camera resource (`SceneWorld.camera()`)
- Camera seam is position-only (`Camera2D`), stored in world resources.
- Camera pan controls: `I/J/K/L`.
- Window resize is handled by renderer surface/buffer resize and alignment stays centered in world space.

## Tools Overlay (Ticket 5)

- Overlay is on-screen by default and updates every frame.
- Toggle overlay visibility with `F3`.
- Current lines:
  - `FPS: ...`
  - `TPS: ...`
  - `Frame: ... ms`
  - `Entities: ...`
  - `Content: loaded` (reserved placeholder for content compile/load status)
- Overlay text blitting is clipping-safe for very small windows and off-screen text.

## Content Pipeline Contract (Ticket 6)

- Ticket 6 is spec-first only (no compiler implementation yet).
- Contract doc: `docs/content_pipeline_contract_v1.md`
- Fixture inputs and expected outcomes: `docs/fixtures/content_pipeline_v1/`
- Locked rules include:
  - runtime consumes compiled `DefDatabase` only (never XML)
  - per-mod `ContentPack v1` cache model
  - `mod_id` sourced from mod folder name
  - `compiler_version` and `game_version` invalidation by exact string match

## Mod Discovery and Compile Plan (Ticket 7)

- Engine builds a deterministic compile plan at startup (no XML parse/compile execution yet).
- Inputs come from `ContentPlanRequest`:
  - ordered enabled mods list (provided by game)
  - `compiler_version`
  - `game_version`
- Deterministic cache paths in v1:
  - `cache/content_packs/<mod_id>.pack`
  - `cache/content_packs/<mod_id>.manifest.json`
- Manifest language is generic contract language; JSON is only the current v1 encoding.
- Ticket 7 reads only the manifest fields (plus pack-file existence), not pack internals.
- Deterministic ordering rules:
  - base is always first (`mod_id = "base"`)
  - enabled mods keep caller order
  - XML files are hashed by normalized relative path + bytes in sorted path order
- Startup logs emit:
  - compile plan summary
  - per-mod decision (`UseCache` or `Compile`) with reason

## XML MVP Compiler (Ticket 8)

- Engine compiles `EntityDef` XML into runtime `DefDatabase` at startup.
- Compilation is strict and fail-fast:
  - malformed XML, unknown fields, missing required fields, and invalid values fail startup
  - errors include mod id, file path, and best-effort line/column
- Runtime uses numeric IDs in `DefDatabase`; string lookup is only for startup archetype resolution.
- Merge rule for duplicate `defName` across mods is last-mod-wins by load order.
- Stable numeric IDs are assigned by sorted `defName`.
- Game loads `proto.player` from `DefDatabase` and uses XML data for:
  - `renderable`
  - `moveSpeed` (defaults to `5.0` if omitted)

## ContentPack Binary Cache (Ticket 9)

- Startup now uses `build_or_load_def_database(...)`:
  - builds compile plan
  - loads valid per-mod packs from cache
  - recompiles only invalid/corrupt mods from XML
- Deterministic cache paths:
  - `cache/content_packs/<mod_id>.pack`
  - `cache/content_packs/<mod_id>.manifest.json`
- Atomic write order per rebuilt mod:
  - write/rename pack first
  - then write/rename manifest
- Manifest remains planning authority in v1, but pack header redundantly stores the same key metadata:
  - format/version fields
  - `mod_id`
  - `mod_load_index`
  - `compiler_version`
  - `game_version`
  - enabled-mods hash and input hash
- Cache load cross-checks:
  - manifest matches expected request/inputs exactly
  - pack decodes and payload hash is valid
  - pack header fields match manifest exactly
- Any cache inconsistency falls back to per-mod rebuild; XML compile errors are still fatal.

## Override Rule MVP (Ticket 10)

- `EntityDef` merge now applies field-level overrides in deterministic load order:
  - scalar fields are last-writer-wins (`label`, `renderable`, `moveSpeed`)
  - list field `tags` replaces the prior list when present
  - omitted fields preserve earlier values
- A full first definition still requires:
  - `defName`
  - `label`
  - `renderable`
- Partial defs are treated as override patches; if no earlier target exists, startup fails with a clear `MissingOverrideTarget` compile error.
- Runtime `EntityArchetype` now includes `tags`.
- Binary pack schema changed for Ticket 10 and cache invalidation is signaled only by `pack_format_version` (no secondary version gate).

### Enabling Mods Without Code Edits

Set ordered enabled mods via `PROTOGE_ENABLED_MODS` (comma-separated):

PowerShell:

```powershell
$env:PROTOGE_ENABLED_MODS="betterlabels,replacetags"
cargo run
```

Bash/zsh:

```bash
export PROTOGE_ENABLED_MODS="betterlabels,replacetags"
cargo run
```

## Scenes and Entities

- Two hardcoded in-memory scenes are active.
- Press `Tab` to switch between Scene A and Scene B at runtime.
- Scene switching performs lifecycle in order: `unload -> clear -> load`.
- Entity model is engine-owned:
  - `EntityId` (session-unique)
  - `Transform` (`position` + optional `rotation_radians`)
  - renderer-agnostic `RenderableDesc`
- Game rules read an engine `InputSnapshot` in `Scene.update(...)`, so game code has no `winit` dependency.

## Slow Frame Simulation (Manual Test)

Use this to force an artificial per-frame delay and verify sim clamping behavior:

PowerShell:

```powershell
$env:PROTOGE_SLOW_FRAME_MS="250"
cargo run
Remove-Item Env:PROTOGE_SLOW_FRAME_MS
```

Bash/zsh:

```bash
export PROTOGE_SLOW_FRAME_MS="250"
cargo run
unset PROTOGE_SLOW_FRAME_MS
```

## Optional Override

PowerShell:

```powershell
$env:PROTOGE_ROOT="C:\path\to\Prototype Game Engine"
cargo run
```

Bash/zsh:

```bash
export PROTOGE_ROOT="/path/to/Prototype Game Engine"
cargo run
```

## Troubleshooting

If you see a root-detection error, set `PROTOGE_ROOT` to the repo root and rerun.
