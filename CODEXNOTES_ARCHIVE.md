# CODEXNOTES_ARCHIVE.md
## Purpose
Historical record for decisions, ticket-by-ticket logs, superseded notes, and implementation timelines.
Use `CODEXNOTES.md` for current active context.
## Migration Notes
- On 2026-02-20, historical notes were moved from `CODEXNOTES.md` to enforce living-vs-archive separation in `AGENTS.md`.
- Ticket notes below are preserved as moved, in chronological order.
---
## Ticket Notes (2026-02-18)
- Ticket 0 implemented: workspace bootstrap + deterministic startup path resolution.
- New workspace layout:
  - `Cargo.toml` workspace root with members `crates/engine` and `crates/game`
  - `crates/engine`: startup/bootstrap contracts and path resolution
  - `crates/game`: binary entrypoint, banner, logging, clean exit behavior
  - Canonical directories created: `assets/base/`, `mods/`, `docs/`
- Startup path contract added in code (`crates/engine/src/lib.rs`):
  - `AppPaths { root, base_content_dir, mods_dir, cache_dir }`
  - `resolve_app_paths() -> Result<AppPaths, StartupError>`
- Root resolution rules (locked for now):
  - If `PROTOGE_ROOT` is set: validate as repo marker root
  - Else walk upward from executable directory and choose first marker match
  - Marker definition: directory containing `Cargo.toml` and either `crates/` or `assets/`
  - Never resolve from current working directory
- Failure UX contract:
  - If root cannot be found, fail fast with explicit instructions to set `PROTOGE_ROOT` and include PowerShell + Bash examples.
- Runtime directory behavior:
  - `cache/` is runtime-managed and created via `create_dir_all` at startup.

## Pitfalls and Gotchas (Update as found)
- Local environment pitfall: `cargo` command may be unavailable in shell PATH even when repo scaffold is valid. Validation commands could not be executed in this session.

## Change Log (Optional)
- 2026-02-18: Ticket 0 scaffolded workspace crates and deterministic root discovery from executable ancestors with `PROTOGE_ROOT` override.

---

## Ticket Notes (2026-02-19, Ticket 17)
- Renderable contract expanded for sprite v0:
  - `RenderableKind::{Placeholder, Sprite(String)}`
  - XML renderable format supports `Sprite:<key>`
- Sprite key validation contract (shared across compiler + renderer):
  - allowed chars: `a-z`, `0-9`, `_`, `/`, `-`
  - rejected: empty keys, leading `/`, any `\`, any `..`
- Renderer asset lookup seam added:
  - `asset_root` is passed from loop runner into renderer
  - sprite file path is resolved strictly as `asset_root/base/sprites/<key>.png`
  - cache stores sprite decode result by key to avoid repeated decode work
- Fallback behavior locked:
  - invalid sprite key, missing file, or decode failure renders placeholder square
  - frame render continues; no runtime XML parsing is introduced

---

## Ticket Notes (2026-02-19, Ticket 18)
- SceneWorld tilemap state added for visual ground context:
  - `Tilemap { width, height, origin, tiles: Vec<u16> }`
  - `SceneWorld::{set_tilemap, clear_tilemap, tilemap}`
- Tilemap origin convention is explicit and enforced:
  - `origin` is tile `(0,0)` bottom-left corner in world space
  - tile center is `origin + (x + 0.5, y + 0.5)`
- Clear behavior contract:
  - `SceneWorld::clear()` does not clear tilemap
  - tilemap persists until `clear_tilemap()` is called
- Render order contract updated:
  - `clear -> tilemap -> grid debug -> entities -> overlay`
- Renderer v0 tile mapping (engine-owned constants):
  - tile id `0` -> `tile/grass`
  - tile id `1` -> `tile/dirt`
  - unresolved/missing tiles fallback to solid ground color

---

## Ticket Notes (2026-02-19, Ticket 19)
- Interaction affordance v0 added with SceneWorld-owned transient visual state:
  - `SceneVisualState { selected_actor, hovered_interactable }`
  - IDs are weak references; renderer skips unresolved IDs.
- Timed debug markers added to SceneWorld:
  - `DebugMarker { kind, position_world, ttl_seconds }`
  - v0 marker kind: `Order`
  - order marker TTL constant in game: `0.75s`
- Marker ticking contract:
  - single-pass decrement + retain (`retain_mut`) each sim tick
  - expired markers removed when `ttl_seconds <= 0`
- Renderer affordance pass added after entities and before overlay:
  - selected actor outline highlight
  - hovered interactable outline highlight
  - order marker cross at target world position
- Game update contract:
  - hover highlight source is `pick_topmost_interactable_at_cursor(...)` each tick
  - selected visual is cleared when stale/non-actor and only set for live actors

---

## Ticket Notes (2026-02-19)
- Ticket 1 implemented: engine heartbeat and lifecycle loop with fixed-timestep simulation and decoupled metrics surface.
- App/Loop contract added in `crates/engine/src/app/`:
  - `Scene` trait: `update(fixed_dt_seconds)` and `render()`
  - `LoopConfig` for window/timing/tick clamp configuration
  - `run_app(config, scene)` and `run_app_with_metrics(config, scene, metrics_handle)`
  - `MetricsHandle` + `LoopMetricsSnapshot { fps, tps, frame_time_ms }`
- Logging contract:
  - `game` initializes `tracing_subscriber` in `crates/game/src/main.rs`
  - `engine` emits `tracing` events only and does not initialize subscribers
- Loop behavior rules (locked for now):
  - Fixed timestep at configurable TPS (default 60)
  - Render cadence separate from simulation tick cadence
  - Frame delta clamped (`max_frame_delta`) before accumulation
  - Tick work capped per frame (`max_ticks_per_frame`)
  - Remaining runaway backlog dropped with `sim_clamp_triggered` warning log
- Input/lifecycle:
  - Quit via window close or `Esc` key
  - Clean shutdown emits `shutdown` log
- Test hook:
  - `PROTOGE_SLOW_FRAME_MS` adds artificial per-frame delay to verify clamp behavior.

---

## Ticket Notes (2026-02-19, Ticket 2)
- Scene boundary upgraded to explicit lifecycle in `crates/engine/src/app/scene.rs`:
  - `load(world)`, `update(fixed_dt_seconds, input, world) -> SceneCommand`, `render(world)`, `unload(world)`
- Engine-owned minimal input seam added:
  - `InputSnapshot { quit_requested, switch_scene_pressed }`
  - Game detects `Tab` scene-switch intent via snapshot, with no `winit` dependency in game code.
- Entity model and storage added in engine:
  - `EntityId`, `Transform { position, rotation_radians }`, `RenderableDesc`
  - `SceneWorld` owns entities and spawn/despawn queues
  - `EntityId` allocation is monotonic and session-global (no reuse after scene switches)
- Runtime scene switching contract:
  - `SceneCommand::{None, SwitchTo(SceneKey)}`
  - `SceneMachine` manages two in-memory scenes (`A`, `B`)
  - Switch lifecycle order locked: `unload -> world.clear -> load`
- Game wiring (`crates/game/src/main.rs`):
  - Hardcoded Scene A and Scene B
  - `Tab` toggles between scenes at runtime
  - Per-scene entity counts reset on each switch (A=3, B=5)

---

## Ticket Notes (2026-02-19, Ticket 3)
- Input mapping contract added in engine:
  - `InputAction::{MoveUp, MoveDown, MoveLeft, MoveRight, Quit}`
  - `InputSnapshot::is_down(action)` for scene-side action reads
  - Key mapping in loop runner: `WASD` + arrow keys for movement, `Esc` for quit
- Game/engine seam for window title:
  - `Scene::debug_title(&SceneWorld) -> Option<String>` added with default `None`
  - Engine applies title to window when changed; game never touches window handles
- Scene world helpers added:
  - `find_entity(id)` and `find_entity_mut(id)` for controllable entity updates
- Game movement behavior:
  - One controllable player entity per scene
  - Movement speed locked to `5.0` units/second in fixed-timestep update
  - Diagonal movement normalized to keep stable movement magnitude

---

## Ticket Notes (2026-02-19, Ticket 4)
- Minimal rendering path added in engine with `pixels` backend:
  - `crates/engine/src/app/rendering/renderer.rs`
  - `crates/engine/src/app/rendering/transform.rs`
- Camera seam added as world resource:
  - `Camera2D { position }` stored in `SceneWorld`
  - access via `SceneWorld::camera()` and `SceneWorld::camera_mut()`
  - `SceneWorld::clear()` resets camera to default
- Rendering contract locked:
  - Renderer consumes `SceneWorld.entities()` + `SceneWorld.camera()`
  - No new render-specific methods were added to `Scene` trait
  - World-to-screen transform:
    - `screen_x = (world_x - camera_x) * px_per_world + width/2`
    - `screen_y = height/2 - (world_y - camera_y) * px_per_world`
- Resize behavior:
  - `WindowEvent::Resized` and `ScaleFactorChanged` trigger renderer resize of surface/buffer
- Input mapping expanded:
  - Camera pan actions `CameraUp/Down/Left/Right` mapped to `I/J/K/L`
- Game integration:
  - Camera is moved from scene update using action input
  - Debug title includes player and camera position

---

## Ticket Notes (2026-02-19, Ticket 5)
- Tools overlay added in engine (`crates/engine/src/app/tools/overlay.rs`), drawn from renderer each frame.
- Overlay toggle contract:
  - `F3` toggles visibility via engine-owned edge trigger in `InputCollector`
  - hold does not spam toggles; only key-down edge flips state
- Overlay data contract (engine-owned, no scene/window coupling):
  - FPS/TPS/frame time from `MetricsHandle`
  - entity count from `SceneWorld::entity_count()`
  - reserved content status line currently hardcoded as `Content: loaded`
- Glyph system intentionally constrained:
  - only supports required current charset (digits, `.`, `:`, space, `-`, and letters needed for labels/status)
  - unknown characters are treated as space (safe fallback)
- Safety rule locked:
  - text blitter is clipping-safe by design (negative/off-screen coords and tiny viewports never panic or write out of bounds)

---

## Ticket Notes (2026-02-19, Ticket 6)
- Added spec doc: `docs/content_pipeline_contract_v1.md`.
- Added fixture set and expectation matrix:
  - `docs/fixtures/content_pipeline_v1/`
  - `docs/fixtures/content_pipeline_v1/EXPECTATIONS.md`
- Boundary contract locked:
  - XML authoring -> compiled per-mod `ContentPack v1` -> runtime `DefDatabase`
  - runtime simulation must never parse XML
- `mod_id` source rule locked:
  - mods use leaf folder name under `mods/`
  - base content uses `mod_id = "base"` from `assets/base`
- Cache invalidation rules clarified:
  - `compiler_version` and `game_version` are exact-match strings (byte-for-byte equality)
  - any string difference invalidates cache and requires rebuild
- Deterministic rules documented:
  - path-normalized lexical file ordering
  - document-order read
  - `(def_type, def_name)` serialization/ID ordering
- Override behavior locked:
  - scalar fields are last-writer-wins
  - list fields are full replacement (no append/deep merge in v1)

---

## Ticket Notes (2026-02-19, Ticket 7)
- Added engine content planning module:
  - `crates/engine/src/content/{mod.rs,types.rs,discovery.rs,hashing.rs,manifest.rs,planner.rs}`
- Startup now builds deterministic compile plan before scene load via:
  - `build_compile_plan(app_paths, content_plan_request)`
- New public request/output contracts:
  - `ContentPlanRequest { enabled_mods, compiler_version, game_version }`
  - `CompilePlan`, `ModCompileDecision`, `CompileAction`, `CompileReason`, `ContentStatusSummary`
- Deterministic discovery/hash rules implemented:
  - base always first (`mod_id = "base"`)
  - enabled mods use caller-provided order
  - per-mod XML input hash uses normalized relative path + file bytes with sorted path order
- Cache path and manifest rules (v1):
  - pack path: `cache/content_packs/<mod_id>.pack`
  - manifest path: `cache/content_packs/<mod_id>.manifest.json`
  - manifest is durable terminology; JSON is current v1 encoding only
- Ticket 7 validation reads manifest fields only plus pack-file existence check; no pack parsing.
- Exact-match invalidation reaffirmed in planner:
  - `compiler_version` and `game_version` use byte-for-byte string equality

---

## Ticket Notes (2026-02-19, Ticket 8)
- Added XML MVP compiler and runtime def storage:
  - `crates/engine/src/content/compiler.rs`
  - `crates/engine/src/content/database.rs`
- New content runtime contracts:
  - `DefDatabase`, `EntityDefId`, `EntityArchetype`
  - `compile_def_database(app_paths, request) -> Result<DefDatabase, ContentCompileError>`
- Strict XML validation implemented for `EntityDef`:
  - root must be `<Defs>`
  - required fields: `defName`, `label`, `renderable`
  - optional `moveSpeed` with default `5.0`
  - unknown fields fail compile
  - duplicate fields fail compile
  - same-mod duplicate `defName` fails compile
  - cross-mod duplicate `defName` merges with last-mod-wins
- Error model implemented:
  - `ContentCompileError { code, message, mod_id, file_path, location }`
  - best-effort line/column from XML parser/node position
- Determinism rules implemented:
  - compile input order follows Ticket 7 deterministic discovery and file sorting
  - runtime IDs assigned in sorted `defName` order
- Engine startup integration:
  - compile runs after compile-plan generation and before scene load
  - compile error fails startup with actionable message
  - `SceneWorld` now stores `DefDatabase` resource and retains it across scene clears
- Game integration:
  - scene load resolves `proto.player` from `DefDatabase`
  - player `renderable` + `moveSpeed` come from compiled XML archetype
  - missing `proto.player` now fails load with actionable panic message

---

## Ticket Notes (2026-02-19, Ticket 9)
- Added cache load/build orchestration:
  - `crates/engine/src/content/pipeline.rs`
  - public API: `build_or_load_def_database(app_paths, request)`
- Added binary pack I/O:
  - `crates/engine/src/content/pack.rs`
  - custom LE `ContentPack v1` format with payload hash
- Added atomic file helpers:
  - `crates/engine/src/content/atomic_io.rs`
  - temp-file + rename writes
- Startup integration switched from XML-only compile to pipeline load/build in:
  - `crates/engine/src/app/loop_runner.rs`
- Atomicity rule implemented (locked):
  - for rebuilt mods, `.pack` is written atomically first
  - `.manifest.json` is written atomically only after pack write succeeds
- Redundant header validation implemented (locked):
  - pack header stores cache-key fields mirrored from manifest
  - cache load requires manifest-to-expected match and header-to-manifest exact match
  - mismatch/corruption triggers per-mod rebuild fallback
- Failure policy:
  - cache read/validation failures are recoverable per mod (recompile)
  - XML compile failures remain fatal and fail startup
  - atomic write failures are fatal for startup

---

## Ticket Notes (2026-02-19, Ticket 10)
- Override merge semantics for `EntityDef` moved from whole-def replacement to field-level patch merge.
- Locked override rules:
  - scalar fields (`label`, `renderable`, `moveSpeed`) are last-writer-wins in mod load order
  - list field (`tags`) is full replacement when provided
  - omitted fields keep previously merged values
- Missing target behavior locked:
  - partial override with no earlier definition now fails fast with `MissingOverrideTarget`
  - error includes mod id + file path + best-effort location
- `label` rule locked:
  - full initial definition still requires `label` and `renderable` (with `defName`)
- Runtime archetype model updated:
  - `EntityArchetype` now carries `tags: Vec<String>`
- Content pack schema updated for optional patch fields + tags.
- Versioning decision locked:
  - cache/schema change is signaled only by `pack_format_version` (single source of truth)
  - no separate header-version bump path is used
- Game startup config QoL:
  - `PROTOGE_ENABLED_MODS` (comma-separated, ordered) now feeds `ContentPlanRequest.enabled_mods`
  - allows override verification without code edits.

---

## Ticket Notes (2026-02-19, Ticket 11)
- Mouse picking and selection seam added end-to-end (engine input -> world query -> game scene state -> overlay).
- Input contract expanded in `InputSnapshot`:
  - `cursor_position_px: Option<Vec2>`
  - `left_click_pressed: bool` (left-press edge, one tick)
  - `window_size: (u32, u32)`
  - builder helpers added for tests and synthetic snapshots:
    - `with_cursor_position_px(...)`
    - `with_left_click_pressed(...)`
    - `with_window_size(...)`
- Runtime entity contract expanded:
  - `Entity { selectable: bool }` public marker added.
  - `SceneWorld::spawn_selectable(...)` added for explicit selectable spawns.
- Picking contract locked:
  - `SceneWorld::pick_topmost_selectable_at_cursor(cursor_px, window_size) -> Option<EntityId>`
  - overlap resolution is explicit and stable: **last applied spawn wins**
  - implemented with monotonic internal `applied_spawn_order` stamp assigned in `apply_pending`
  - rule remains stable after unrelated despawns (regression test added)
- Shared projection seam locked:
  - `world_to_screen_px(camera, window_size, world_pos)` is now the shared helper for both renderer and picking.
  - exported via engine public API.
- Scene debug seam expanded:
  - `Scene::debug_selected_entity(&self) -> Option<EntityId>` default method added.
  - loop/overlay now reads active scene selection through `SceneMachine`.
- Overlay contract expanded:
  - extra line: `Sel: <id>` or `Sel: none`.
- Game integration:
  - `GameplayScene` now maintains `selected_entity: Option<EntityId>`.
  - left click selects entity under cursor; empty click clears selection.
  - gameplay scene now spawns 3 selectable entities for manual verification.

---

## Ticket Notes (2026-02-19, Ticket 12)
- Added right-click move-order loop with runtime order state stored on entities (not scene-local maps).
- Input contract expanded in `InputSnapshot`:
  - `right_click_pressed: bool` (right-press edge, one tick)
  - builder helper: `with_right_click_pressed(...)`
- Rendering transform seam expanded:
  - `screen_to_world_px(camera, window_size, screen_px)` added as strict inverse of `world_to_screen_px(...)`.
  - exported from `app/rendering/mod.rs`; re-exported upward for current game usage.
- Runtime entity contract expanded in engine:
  - `Entity { actor: bool, move_target_world: Option<Vec2> }`
  - `SceneWorld::spawn_actor(...)` helper added
  - `SceneWorld::entities_mut()` added for scene-side fixed-tick order stepping
- Scene debug seam expanded:
  - `Scene::debug_selected_target(&SceneWorld) -> Option<Vec2>` default method added
  - `SceneMachine` passthrough now provides selected target for overlay
- Overlay contract expanded:
  - new line shows selected actor target as `Target: <x>,<y>` or `Target: idle`
- Gameplay behavior locked for Ticket 12:
  - right click with no selection is a no-op
  - right click only issues move target if selected entity exists and `actor == true`
  - fixed-tick movement steps entities with `actor == true && move_target_world.is_some()`
  - target is cleared on arrival threshold

---

## Ticket Notes (2026-02-19, Ticket 13)
- Added minimal interactable + timed job completion loop using runtime data only (no XML parsing at runtime).
- Content usage (tags-only, no schema change):
  - `assets/base/defs.xml` now includes `proto.resource_pile` with tags:
    - `interactable`
    - `resource_pile`
  - runtime resolves and validates this def via `DefDatabase`.
- Engine runtime entity contract expanded:
  - `Entity { interactable: Option<Interactable>, job_state: JobState, interaction_target: Option<EntityId> }`
  - new runtime types in `scene.rs`:
    - `Interactable { kind, interaction_radius, remaining_uses }`
    - `InteractableKind::ResourcePile`
    - `JobState::{Idle, Working { target, remaining_time }}`
- Engine world query seam expanded:
  - `SceneWorld::pick_topmost_interactable_at_cursor(cursor_px, window_size) -> Option<EntityId>`
  - uses same placeholder screen bounds and deterministic last-applied-spawn tie-break as selection.
- Scene debug/overlay seam expanded:
  - `Scene::debug_resource_count() -> Option<u32>` default method
  - `SceneMachine::debug_resource_count_active()`
  - overlay now includes `items: <count>` (defaults to `0` when absent)
- Gameplay behavior locked for Ticket 13:
  - right-click is interactable-first:
    - hit interactable: actor gets move target to object + `interaction_target`
    - miss interactable: regular ground move target
  - actor starts working once within interaction radius
  - job duration is fixed-step deterministic (`2.0s`)
  - completion increments resource counter and decrements pile uses
  - pile despawns when uses reach zero
  - missing/stale interaction targets are cleared safely to `Idle` state
- Performance-minded loop detail:
  - gameplay reuses scratch vectors (`interactable_cache`, `completed_target_ids`) to avoid per-tick allocation churn.

---

## Ticket Notes (2026-02-19, Ticket 14)
- Added debug inspect snapshot seam from scene to overlay:
  - engine scene types:
    - `DebugInfoSnapshot`
    - `DebugJobState`
  - `Scene` trait now includes:
    - `debug_info_snapshot(&SceneWorld) -> Option<DebugInfoSnapshot>` (default `None`)
  - `SceneMachine` passthrough:
    - `debug_info_snapshot_active(&SceneWorld) -> Option<DebugInfoSnapshot>`
- Overlay contract expanded:
  - `OverlayData` now carries `debug_info: Option<DebugInfoSnapshot>`
  - overlay renders inspect block lines when available:
    - `Inspect`
    - `sel: ...`
    - `pos: ...`
    - `ord: ...`
    - `job: ...`
    - `cnt e/a/i/r: ...`
- Gameplay integration:
  - `GameplayScene` implements `debug_info_snapshot` from selected entity runtime fields:
    - `move_target_world`
    - `job_state`
    - entity transform
  - counts included in snapshot:
    - entities / actors / interactables / items(resource_count)
- Safety/perf notes:
  - overlay draw path remains clipping-safe
  - unknown glyphs remain safe space fallback
  - glyph required-char set expanded minimally for new inspect labels (`I`, `p`, `c`, `j`, `b`, `w`, `k`, `/`)

---

## Ticket Notes (2026-02-19, Ticket 14.5)
- Scene switch semantics changed to persistent runtime ownership in engine:
  - `SceneMachine` now owns per-scene runtime slots:
    - `scene`
    - `world`
    - `is_loaded`
  - normal `SwitchTo` is non-destructive:
    - only changes active pointer
    - loads target once if not loaded yet
    - never unloads/clears on normal switch
- Inactive scene worlds are paused by loop behavior:
  - only active scene/world are ticked in fixed-step update
  - movement/jobs/timers in inactive scenes do not advance
- Hard reset path added:
  - `SceneCommand::HardResetTo(SceneKey)`
  - target runtime path: `unload -> world.clear() -> load -> activate`
  - `SceneWorld::clear()` retaining `DefDatabase` is relied on for reset reload
- Loop ownership seam changed:
  - `loop_runner` no longer owns a standalone `SceneWorld`
  - loop now uses scene-machine active-world accessors for update/apply/render/overlay/debug
- Shutdown contract added:
  - `SceneMachine::shutdown_all()` unloads each loaded scene once, then clears worlds.

---

## Ticket Notes (2026-02-19, Ticket 15)
- Added renderer-local world-space grid debug pass in `crates/engine/src/app/rendering/renderer.rs`.
- Draw order is now:
  - clear
  - grid
  - entity placeholders
  - overlay
- Grid implementation details:
  - uses existing `world_to_screen_px(...)` camera projection seam
  - computes visible world bounds from camera + viewport and iterates only visible line indices
  - supports minor lines + major lines every 5 cells
  - major line classification uses Euclidean modulo (`rem_euclid`) for deterministic negative-index behavior
  - uses clipping-safe per-pixel writes with checked indexing
  - handles zero/tiny viewports without panic
- No content pipeline, renderable type, or public API contract changes.

---

## Ticket Notes (2026-02-19, Ticket 16)
- Added save/load v0 key seam in engine input:
  - `F5` save edge
  - `F9` load edge
  - `InputSnapshot` now includes `save_pressed` and `load_pressed` getters/builders.
- Save/load implementation is game-local (no new engine-wide save API).
- Save file contract:
  - JSON format with `save_version` integer
  - per-scene files under `cache/saves/`:
    - `scene_a.save.json`
    - `scene_b.save.json`
- Schema is runtime-focused:
  - camera position
  - entity runtime state (transform, selectable/actor, move target, interaction target, job state, interactable runtime fields)
  - selected/player entity refs and `resource_count`
  - references use **saved entity indices** for remap.
- Load safety contract:
  - parse, version, scene key, and all index refs are validated **before** clearing/mutating world/scene state.
- Reconstruction contract:
  - static renderable/config is rebuilt from defs/archetypes when needed
    - actors -> `proto.player` archetype renderable/move speed
    - interactables -> `proto.resource_pile` archetype renderable
  - non-role entities use placeholder renderable fallback.

---

## Ticket Notes (2026-02-19, Hotfix 16.1)
- Added authoritative render FPS cap knob in loop config:
  - `LoopConfig.max_render_fps: Option<u32>`
  - default is `None` (cap off)
  - zero values normalize to cap-off.
- Render pacing contract:
  - exactly one FPS-cap sleep point in redraw loop (`compute_cap_sleep` + `thread::sleep`)
  - cap helpers added and unit-tested:
    - `target_frame_duration`
    - `compute_cap_sleep`
    - cap normalization.
- `PROTOGE_SLOW_FRAME_MS` retained as explicit debug delay:
  - still applies separate intentional sleep
  - explicitly documented in code as not an FPS cap.
- Overlay FPS line changed to show cap and debug delay together:
  - format: `[{Current} / {Cap}] dbg+{slow_ms}ms`
  - cap-off display uses ASCII `"off"` to avoid font/glyph ambiguity on overlay text rendering.
- Startup logs now include effective render cap and debug delay values for visibility.
- Important runtime note:
  - backend present/vsync behavior may still bound observed FPS independently of app-level cap.

---

## Ticket Notes (2026-02-19, Hotfix 16.2)
- Fixed click-picking regression for sprite-backed entities in crates/engine/src/app/scene.rs.
- Root cause: pick helpers (pick_topmost_selectable_at_cursor, pick_topmost_interactable_at_cursor) filtered to RenderableKind::Placeholder only, so entities rendered as RenderableKind::Sprite(...) were never pickable.
- Behavioral contract update:
  - picking now considers both placeholder and sprite renderables
  - existing spawn-order tie-break semantics are unchanged.
- Added regression tests:
  - `pick_topmost_selectable_includes_sprite_renderables`
  - `pick_topmost_interactable_includes_sprite_renderables`
- Pitfall pattern:
  - avoid gameplay/input logic branching on debug/fallback render type; renderable variants can evolve without changing selection semantics.

---

## Ticket Notes (2026-02-19, Debug Perf Tuning)
- Added conservative dev-profile optimization settings in workspace `Cargo.toml`:
  - `[profile.dev] opt-level = 1`
  - `[profile.dev.package."*"] opt-level = 2`
- Purpose:
  - improve debug runtime responsiveness/FPS without switching to full release mode
  - keep debuginfo and normal debug behavior intact.

---

## Ticket Notes (2026-02-19, Ticket 20)
- Camera zoom contract added for MVP discrete zoom:
  - `Camera2D { position, zoom }`
  - constants: `CAMERA_ZOOM_DEFAULT=1.0`, `CAMERA_ZOOM_MIN=0.5`, `CAMERA_ZOOM_MAX=2.0`, `CAMERA_ZOOM_STEP=0.1`
  - methods: `set_zoom_clamped`, `apply_zoom_steps`, `effective_zoom`
- Input zoom contract (engine loop/input seam):
  - `InputSnapshot.zoom_delta_steps: i32`
  - `InputCollector` only accumulates pending zoom steps from mouse wheel + zoom keys
  - zoom keys are edge-triggered only (`=`/`-` and numpad add/subtract), no held-repeat behavior
  - `snapshot_for_tick` copies and resets pending zoom steps each tick
- Ownership rule locked:
  - camera zoom mutation occurs only in `GameplayScene::update` via `world.camera_mut().apply_zoom_steps(...)`
  - mutation is applied before any `screen_to_world_px(...)` conversions in update
- Projection math contract:
  - `world_to_screen_px` and `screen_to_world_px` both use `camera_pixels_per_world(camera)` and remain inverse within pixel-rounding tolerance
- Save/load contract bumped to v2:
  - `SAVE_VERSION = 2`
  - `SaveGame` now stores `camera_zoom`
  - load validation rejects non-finite zoom
  - load applies zoom through clamped setter

---

## Ticket Notes (2026-02-20, Ticket 21)
- Save/load reference contract moved from index-based links to stable save IDs.
- Save schema bumped to v3:
  - `SAVE_VERSION = 3`
  - `SavedEntityRuntime.save_id: u64` is required
  - refs now use save IDs:
    - `interaction_target_save_id`
    - `SavedJobState::Working { target_save_id, ... }`
    - `selected_entity_save_id`
    - `player_entity_save_id`
  - `SaveGame.next_save_id: u64` is persisted and validated
- Gameplay-owned save ID runtime state added in `crates/game/src/main.rs`:
  - `GameplayScene.entity_save_ids: HashMap<EntityId, u64>`
  - `GameplayScene.next_save_id: u64`
- Sync guarantees locked:
  - `sync_save_id_map_with_world` only assigns IDs to missing live entities
  - existing entity->save_id mappings are never reassigned
  - missing assignments are deterministic by sorted `EntityId`
  - stale mappings are dropped when entities despawn
- Load restore guarantees:
  - entity save-id map is rebuilt from loaded save IDs
  - `next_save_id` is restored from `SaveGame.next_save_id`
  - validation still occurs before world mutation (no partial-apply on invalid save)

---

## Ticket Notes (2026-02-20, Ticket 22)
- Content authoring renderable contract extended with attribute-based XML form:
  - `<renderable kind="Sprite" spriteKey="player" />`
  - `<renderable kind="Placeholder" />`
- Backward compatibility retained for legacy text renderables:
  - `Placeholder`
  - `Sprite:<key>`
- Parsing precedence/validation rules locked:
  - if `kind` attribute is present, attribute mode is used
  - `kind` + non-whitespace text in `<renderable>` is invalid
  - unknown `<renderable>` attributes are compile errors
  - `kind="Sprite"` requires `spriteKey`
  - `kind="Placeholder"` must not include `spriteKey`
- Runtime/pipeline seam unchanged:
  - compiler still produces `RenderableKind`
  - pack/database/runtime continue consuming compiled data only (no runtime XML parsing)

---

## Ticket Notes (2026-02-20, Ticket 23)
- Loop runner window ownership cleanup:
  - removed leaked window lifetime hack (`Box::leak`) from `crates/engine/src/app/loop_runner.rs`
  - window is now created as `Arc<Window>` and shared between event loop and renderer
- Renderer lifetime contract updated:
  - `Renderer` is now non-generic (no window lifetime parameter)
  - `Renderer::new` now accepts `Arc<Window>` and owns it internally
  - pixels surface is rebuilt on resize from owned window handle
- Public app loop API unchanged:
  - `run_app` and `run_app_with_metrics` signatures are unchanged
- Fallback strategy rule (locked):
  - if `Arc<Window>` -> `Pixels<'static>` ever fails to typecheck in `pixels 0.15`, switch to a safe self-referential container approach (for example `self_cell`)
  - manual unsafe self-reference is not allowed

---

## Ticket Notes (2026-02-20, Ticket 24)
- Frame pacing/cap contract formalized from hotfix work:
  - one authoritative cap knob on loop config is `LoopConfig.fps_cap: Option<u32>`
  - `Some(0)` normalizes to uncapped (`None`)
  - redraw loop keeps a single FPS-cap sleep point (`compute_cap_sleep` -> `thread::sleep`)
  - debug slow-frame delay remains explicit and separate (`dbg+...ms`)
- Uncapped display/log contract:
  - overlay FPS line shows `∞` when uncapped: `[{current} / ∞]`
  - startup `loop_config` cap logging also renders uncapped as `∞`
- Hotfix 16.1 naming superseded:
  - previous field name `LoopConfig.max_render_fps` is deprecated by this ticket and removed from runtime config API.

---

## Ticket Notes (2026-02-20, Docs Sync)
- Documentation drift cleanup completed for current runtime baseline:
  - `docs/PROTOGE_INFRASTRUCTURE_REFERENCE.md` rewritten to reflect Tickets 0-24 contracts and current runtime ownership/seams.
  - `README.md` refreshed to match current controls/features and to point deeper details to the infrastructure reference.
- Key doc corrections:
  - removed stale `Box::leak` window-lifetime note
  - updated save/load references from index-based v0 wording to stable save-id v3 wording
  - updated FPS cap naming to `LoopConfig.fps_cap` and uncapped display/log to `∞`.

---

## Ticket Notes (2026-02-20, Ticket 25)
- Added engine-owned perf counters in `crates/engine/src/app/tools/perf_stats.rs`:
  - `PerfStats` (always-on internally for MVP)
  - `PerfStatsSnapshot { sim, ren }`
  - `RollingMsStats { last_ms, avg_ms, max_ms }`
  - fixed rolling window size: 120 samples, O(1) push with running-sum average over current sample count only.
- Loop timing boundaries are explicit in `crates/engine/src/app/loop_runner.rs` comments:
  - `sim_ms`: starts immediately before fixed-step tick loop; ends after tick work + scene switch handling + backlog clamp handling.
  - `render_ms`: starts immediately before `scenes.render_active()`; ends immediately after `renderer.render_world(...)` returns.
  - `render_ms` excludes FPS cap sleep and non-render housekeeping.
- Overlay integration in `crates/engine/src/app/tools/overlay.rs`:
  - New compact lines: `SIM l/a/m` and `REN l/a/m` with `x.xx/x.xx/x.xx ms` formatting.
  - Perf snapshot is only read/passed when overlay is visible (`OverlayData` construction remains gated by `overlay_visible.then(...)`).
- Startup logging now emits one line for perf default config (`perf_stats_config`).
- Glyph set expanded minimally for uppercase `M`, `N`, `R` to support `SIM/REN` labels.
- Added tests:
  - rolling window math and eviction/max behavior (`perf_stats.rs`)
  - overlay perf line formatting and updated layout/glyph coverage (`overlay.rs`).

---

## Ticket Notes (2026-02-20, Ticket 26)
- Added soft performance budget config fields to `LoopConfig` in `crates/engine/src/app/loop_runner.rs`:
  - `sim_budget_ms: Option<f32>`
  - `render_budget_ms: Option<f32>`
  - default for both is `None` (disabled).
- Budget normalization contract:
  - only finite positive values are active budgets.
  - `None`, `<= 0`, `NaN`, and `Infinity` are treated as disabled.
- Added loop-owned consecutive breach gate logic:
  - `SoftBudgetWarningGate` with fixed `K=3` consecutive breach threshold.
  - breach condition is strict `last_ms > threshold_ms`.
  - warning latches once per sustained streak and resets on recovery (`last_ms <= threshold_ms`).
- Added startup config log event: `perf_budget_config` with normalized budget values and `consecutive_breach_frames`.
- Added runtime warning event: `perf_budget_exceeded` including path (`sim`/`render`), threshold, current `last/avg/max`, and consecutive breach counts.
- No runtime behavior changes beyond logging; no sim/render control flow changes.

---

## Ticket Notes (2026-02-20, Ticket 27)
- Renderer culling added in `crates/engine/src/app/rendering/renderer.rs` using one per-frame world bounds snapshot:
  - `view_bounds_world(camera, window_size, padding_px)` computed once in `render_world`.
  - Culling padding is applied by expanding view bounds from `16px` only.
- Entity culling contract:
  - fixed conservative radius in world tile units (`ENTITY_CULL_RADIUS_WORLD_TILES = 0.5`).
  - padding is not added into entity radius.
- Affordance culling added (selected actor, hovered interactable, order markers) against shared view bounds.
- Tilemap culling contract:
  - visible rect computed via floor/ceil-minus-one with inclusive clamping:
    - `x_min = floor(min_x - origin.x)`
    - `x_max = ceil(max_x - origin.x) - 1`
    - `y_min = floor(min_y - origin.y)`
    - `y_max = ceil(max_y - origin.y) - 1`
  - then clamped to map bounds; `None` when no overlap.
- Safety and correctness:
  - negative coordinate cases covered.
  - tiny viewport cases covered with finite-safe bounds math.
- Added renderer unit tests for:
  - view bounds math and tiny viewport safety
  - tile visible-rect formula/clamping/outside-map handling
  - point-radius culling behavior in negative coordinates

---

## Ticket Notes (2026-02-20, Ticket 28)
- Runtime order seam consolidated to `OrderState` in `crates/engine/src/app/scene.rs`:
  - `Idle`
  - `MoveTo { point: Vec2 }`
  - `Interact { target_save_id: u64 }`
  - `Working { target_save_id: u64, remaining_time: f32 }`
- Engine `Entity` runtime fields migrated:
  - removed: `move_target_world`, `interaction_target`, `job_state`
  - added: `order_state: OrderState`
- Public exports updated:
  - `OrderState` is re-exported from `crates/engine/src/app/mod.rs` and `crates/engine/src/lib.rs`
  - `JobState` export removed.
- Gameplay save-id ownership contract in `crates/game/src/main.rs`:
  - keeps both maps for O(1) target resolution:
    - `entity_save_ids: HashMap<EntityId, u64>`
    - `save_id_to_entity: HashMap<u64, EntityId>`
  - maps are rebuilt/synced on load/sync paths and cleared on unload/reset paths.
- Target resolution contract:
  - `OrderState` stores only stable `target_save_id` for interact/work states.
  - each tick resolves save-id -> runtime `EntityId` via `save_id_to_entity` and validates entity existence.
  - unresolved or invalid targets force actor `OrderState::Idle`.
- Behavior lock preserved:
  - right-click still prefers interactable target over ground move.
  - `Interact` owns movement toward target until within interaction radius (no split move order stored).
  - on work completion: resource increments, target usage decrements, despawn when empty.
  - save-id mappings are removed when target despawns to keep resolution strict.
- Save/load compatibility lock:
  - on-disk schema remains `SAVE_VERSION = 3` and unchanged fields:
    - `move_target_world`
    - `interaction_target_save_id`
    - `job_state`
  - runtime -> saved conversion maps from `OrderState` to existing v3 fields.
  - saved -> runtime conversion precedence:
    1. `job_state == Working` -> `OrderState::Working`
    2. else `interaction_target_save_id` -> `OrderState::Interact`
    3. else `move_target_world` -> `OrderState::MoveTo`
    4. else `OrderState::Idle`
- Debug seam updated:
  - `debug_selected_target` and `debug_info_snapshot.selected_order_world` now derive from `OrderState` and save-id resolution.
  - `selected_job_state` reports `Working` only for `OrderState::Working`, otherwise `Idle`/`None` as before.
- Test coverage added/updated:
  - migration assertions for `order_state`
  - precedence test for saved->runtime conversion
  - move-order save/load determinism branch test
  - interact workflow save/load mid-work parity test
- Pitfall recorded:
  - `apply_save_game` requires `DefDatabase` present when saved entities include actor/interactable data; tests that restore saves must seed defs first.

---

## Ticket Notes (2026-02-20, Ticket 29)
- Added a test-only determinism regression harness in `crates/game/src/main.rs` (inside `#[cfg(test)]`):
  - scripted fixed-step replay via `TickAction` (`Noop`, `SelectWorld`, `RightClickWorld`)
  - checkpoint capture via `ScriptCheckpoint`
  - replay executor `run_script_and_capture(...)` runs `scene.update(...)` + `world.apply_pending()` per tick.
- SimDigest contract (drift detector) is exact-bit based:
  - camera position/zoom stored as `f32::to_bits()`
  - selected entity represented as stable `selected_save_id`
  - resource count included
  - entities sorted by `save_id` and include:
    - `save_id`
    - `entity_kind` tag (`Actor`, `Interactable`, `Other`) for clearer diffs
    - position bits
    - `OrderDigest` (`Idle`, `MoveTo`, `Interact`, `Working`)
    - interactable remaining uses.
- Projection coupling policy for harness:
  - projection is used only in one helper (`input_for_action`) to convert scripted world targets to click cursor positions.
  - deterministic fixtures explicitly lock camera to fixed state (`position=(0,0)`, `zoom=1.0`) and scripts do not emit camera inputs.
- Added deterministic fixtures:
  - `make_move_fixture()` returns `(scene, world, actor_save_id)`
  - `make_interact_fixture()` returns `(scene, world, actor_save_id, target_save_id)`.
- Added required regression tests:
  - `determinism_script_pure_move_digest_matches_replay`
  - `determinism_script_interact_work_despawn_digest_matches_replay`
- Interactable completion assertion is target-specific:
  - interact determinism test asserts despawn by checking the spawned target’s `target_save_id` is absent from the final digest entity list.

---

## Ticket Notes (2026-02-20, Ticket 30)
- Save/load v3 robustness polish implemented in `crates/game/src/main.rs` with no schema/version changes.
- Parse-stage diagnostics now include JSON field paths:
  - added `GameplayScene::parse_save_game_json(raw)` using `serde_path_to_error` + `serde_json::Deserializer`.
  - parse errors now report `parse save json at <path>: <reason>` when a path is available.
- Validation diagnostics standardized with explicit field paths:
  - helper formatting methods:
    - `validation_err(path, message)`
    - `expected_actual(path, expected, actual)`
  - key validations now report paths such as:
    - `save_version`, `scene_key`, `camera_position.x`, `camera_zoom`, `next_save_id`
    - `entities[i].save_id`
    - `entities[i].interaction_target_save_id`
    - `entities[i].job_state.target_save_id`
- Added finite/number-sanity validation before any world mutation:
  - top-level: `camera_position.{x,y}`, `camera_zoom` must be finite.
  - per-entity: `position.{x,y}`, optional `rotation_radians`, optional `move_target_world.{x,y}` finite.
  - working jobs: `remaining_time` finite and `>= 0`.
  - interactable runtime: `interaction_radius` finite and `>= 0`.
- Validation-first restore rule remains:
  - stale save-id references (selected/player/interaction/job target) are rejected in `validate_save_game` before `apply_save_game`.
  - `apply_save_game` behavior remains unchanged and still assumes validated input.
- Camera restore behavior unchanged by design:
  - zoom still restored via `set_zoom_clamped`.
  - finite camera position now explicitly required during validation.
- Test coverage added for Ticket 30:
  - parse diagnostics for missing required field, unknown enum tag, and type mismatch with path assertions.
  - validation diagnostics for dangling references, non-finite/invalid numbers, and invalid `next_save_id` messages.
  - no-partial-mutation guard test confirms parse/validation failures do not mutate scene/world runtime state.
---

## Ticket Notes (2026-02-20, Ticket 31)
- Hot-loop perf audit fixes applied with no intended behavior changes.
- Gameplay update path (`crates/game/src/main.rs`):
  - cursor interactable pick is now computed once per tick after zoom/marker tick and reused for:
    - right-click interactable-first order assignment
    - hovered interactable visual.
  - picking tie-break behavior is preserved because pick functions are unchanged.
  - interactable-first command semantics are preserved.
- Gameplay scratch reuse expanded:
  - added `GameplayScene.interactable_lookup_by_save_id: HashMap<u64, (EntityId, Vec2, f32)>`
  - cleared/reused each tick.
  - populated during existing `interactable_cache` build pass for active interactables.
  - actor `Interact`/`Working` lookup now uses direct save-id keyed scratch-map access instead of per-actor linear scans.
- Scene despawn apply optimization (`crates/engine/src/app/scene.rs`):
  - `pending_despawns` now sorted + deduped before retain.
  - entity retain uses binary search on sorted pending ids instead of repeated `Vec::contains`.
  - spawn apply ordering unchanged.
- Renderer sprite cache hit-path optimization (`crates/engine/src/app/rendering/renderer.rs`):
  - `resolve_cached_sprite` now uses one `cache.get(key)` fast path on hit with no allocations.
  - miss path still resolves, inserts, then returns cached ref.
- Added regression test:
  - `scene_world_duplicate_pending_despawns_are_safe_and_idempotent` confirms duplicate pending despawns are safe and remove only targeted entity.

## Ticket Notes (2026-02-20, Ticket 32.1)
- Added engine-owned console shell under `crates/engine/src/app/tools/console.rs`.
- Console state/data contract (bounded, ring-buffer style):
  - `is_open`, `current_line`, `history`, `output_lines`, `pending_lines`.
  - bounds: history/output/pending/current-line caps enforced to prevent unbounded growth.
- Input seam update in `crates/engine/src/app/loop_runner.rs`:
  - console toggle uses `Backquote` edge-trigger tracking in `InputCollector`.
  - when console is open, keyboard events route to console edit/submit/navigation handlers.
  - gameplay key/mouse/wheel input is suppressed while console is open.
  - per-tick `InputSnapshot` is neutralized while console is open (no gameplay actions/edges).
- Console behavior:
  - `Backspace` delete, `Enter` submit (echo `> line` + enqueue raw line), `Escape` close+clear.
  - `Up`/`Down` history navigation with draft restore on exit.
  - pending raw submissions are retained in bounded queue for a future consumer.
- Renderer seam update:
  - `Renderer::render_world` now accepts console state and draws console after overlay (`world -> overlay -> console`).
- Overlay text utility update:
  - added `draw_text_clipped_with_fallback(..., fallback_char)` for console text rendering.
  - added `?` glyph so unsupported chars can render as `?` in console output/prompt.

## Ticket Notes (2026-02-20, Ticket 32.2)
- Added parser/registry pipeline module: `crates/engine/src/app/tools/console_commands.rs`.
- Queue contract split:
  - local immediate actions: `help`, `clear`, `echo`.
  - queueable-only `DebugCommand` variants: `ResetScene`, `SwitchScene`, `Quit`, `Despawn`, `Spawn`.
- `help` ordering guarantee:
  - command list output follows registry registration order exactly.
  - no sorting and no hash iteration order dependence.
- Parsing pipeline behavior:
  - drains raw `ConsoleState.pending_lines` each frame.
  - tokenizes (whitespace + simple `\"...\"` quoted tokens).
  - validates args and emits human-readable usage errors.
  - unknown command format: `error: unknown command '<name>'. try: help`.
  - queueable parse success format: `queued: <normalized command text>`.
- Loop integration:
  - `ConsoleCommandProcessor` is loop-owned in `crates/engine/src/app/loop_runner.rs`.
  - processor runs each redraw frame before render; queueable commands are not executed yet.

## Ticket Notes (2026-02-20, Ticket 32.2.1)
- Expanded shared text glyph lookup in `crates/engine/src/app/tools/overlay.rs` so ASCII printable range `32..=126` always maps to a glyph.
- Console prompt and scrollback now render ASCII printable characters without `?` fallback substitutions (console still uses `draw_text_clipped_with_fallback(..., '?')`).
- Fallback behavior remains active for non-ASCII-printable characters (`glyph_for` returns `None` outside ASCII printable).
- Removed the non-ASCII infinity glyph from lookup and switched uncapped FPS text to ASCII `"inf"` for consistent rendering with the restricted glyph set.

## Ticket Notes (2026-02-20, Ticket 32.3)
- Queueable command feedback contract changed:
  - queueable parse success no longer prints `queued: ...`.
  - queueable command user-visible feedback now emits execution-time `ok: ...` or `error: ...` only.
- Boundary/seam contract:
  - `DebugCommand` remains tools-layer (`crates/engine/src/app/tools/console_commands.rs`).
  - scene-facing API in `crates/engine/src/app/scene.rs` now uses separate `SceneDebugCommand` + `SceneDebugContext` + `SceneDebugCommandResult`.
  - mapping is one-way in loop routing: `DebugCommand::{Spawn,Despawn}` -> `SceneDebugCommand::{Spawn,Despawn}`.
- Loop routing/apply timing (`crates/engine/src/app/loop_runner.rs`):
  - drained queueable batch is executed in one place each redraw.
  - `apply_pending_active()` is called once after the batch for scene debug spawn/despawn commands.
  - `apply_pending_active()` is called immediately after active-scene-changing scene-machine operations (`switch_scene` when changed, `reset_scene` always).
- Gameplay scene hook (`crates/game/src/main.rs`):
  - implemented `Scene::execute_debug_command` for spawn/despawn.
  - spawn resolves def by name via `DefDatabase`, chooses position by explicit/cursor/player/origin fallback, and updates save-id maps.
  - despawn uses existing world despawn queue path and removes save-id mappings on success.


---

## Archive Migration (2026-02-23)
- Moved from `CODEXNOTES.md` to keep living notes concise per AGENTS guidance.

## Module Boundaries and Ownership
### A. Module map
#### Core
#### App/Loop
#### SceneMachine and Scene
#### World (SceneWorld and runtime state)
#### Rendering
#### Assets and Content Pipeline
#### Input
#### Tools (Overlay, Console)
#### Placeholders (Physics, Audio, Scripting seam)
### B. Ownership rules
Core owns shared primitive types, IDs, math/value helpers, and cross-cutting error/reporting contracts; Core must not own gameplay rules, runtime scene state, rendering orchestration, or tool command routing; Core may call standard library helpers and internal pure utility helpers; Core must not call App/Loop, SceneMachine/Scene, World, Rendering, Assets/Content, Input, or Tools runtime flows; data it is allowed to mutate is limited to its own local values and internal utility state with no mutation of scene/world runtime data.

App/Loop owns process lifecycle, window/event integration, fixed-timestep timing, frame pacing, and command routing between active scene and engine shell; App/Loop must not own gameplay rules, entity definitions, content compilation decisions, or renderable gameplay state; App/Loop may call SceneMachine and Scene entry points, Input sampling, Rendering entry points, and Tools surface hooks; App/Loop must not call gameplay mutation paths directly except through scene-facing APIs/debug hooks; data it is allowed to mutate is loop-local timing state, routing queues, and lifecycle flags, not gameplay world internals.

SceneMachine and Scene owns scene lifecycle transitions, active-scene selection, per-scene world persistence semantics, gameplay rule execution, and scene-scoped command handling; SceneMachine and Scene must not own low-level loop timing, renderer internals, content file parsing, or platform event pumping; SceneMachine and Scene may call World mutation/apply APIs, read input snapshots, and issue render requests through rendering seam contracts; SceneMachine and Scene must not call platform loop internals or bypass world deferred-apply semantics; data it is allowed to mutate is scene-owned gameplay state and scene-world command intent through defined apply points.

World (SceneWorld and runtime state) owns runtime entity/state storage, deferred mutation queues, camera/world data containers, and deterministic apply points for pending mutations; World (SceneWorld and runtime state) must not own high-level scene policy decisions, loop pacing policy, content authoring sources, or console shell control flow; World (SceneWorld and runtime state) may call internal data-structure helpers and expose read/write APIs consumed by Scene and read-only APIs consumed by Rendering/Tools; World (SceneWorld and runtime state) must not call App/Loop control paths or content compilation pipeline; data it is allowed to mutate is authoritative runtime state and queued/deferred mutation buffers only.

Rendering owns frame composition, camera/world projection, and draw-order execution from world snapshots and overlay/tool inputs; Rendering must not own gameplay decision logic, entity simulation state mutation, scene transition policy, or content compilation; Rendering may call read-only world accessors, camera transforms, and asset lookup/render backends; Rendering must not call scene gameplay mutation APIs or loop command routing mutation paths; data it is allowed to mutate is renderer-local transient buffers/caches and GPU/frame resources, not gameplay world state.

Assets and Content Pipeline owns mod/content discovery, XML authoring compile process, cache/manifest validation, and runtime DefDatabase loading from compiled packs; Assets and Content Pipeline must not own live gameplay runtime mutation, per-frame loop logic, or renderer gameplay decisions; Assets and Content Pipeline may call filesystem I/O, hashing, compiler/planner stages, and pack/database loaders; Assets and Content Pipeline must not call simulation update loops or parse XML at runtime during simulation; data it is allowed to mutate is content cache artifacts, compile outputs, and content database state during load/refresh boundaries.

Input owns raw input sampling normalization into deterministic frame/tick snapshots and action-mapping state used by loop/scene update calls; Input must not own gameplay rule execution, scene switching policy, renderer behavior, or content compilation; Input may call platform input event adapters and provide snapshots to App/Loop and Scene; Input must not call gameplay mutation functions directly or invoke scene command handlers on its own; data it is allowed to mutate is input collector/internal edge-trigger state and snapshot buffers.

Tools (Overlay, Console) owns debug UI text/metrics presentation and console command parsing/dispatch wiring for developer workflows; Tools (Overlay, Console) must not own core gameplay systems, loop timing ownership, renderer architecture policy, or content pipeline policy; Tools (Overlay, Console) may call read-only inspection paths and scene debug hook interfaces for explicit debug mutations; Tools (Overlay, Console) must not call loop runner gameplay mutation paths directly or bypass scene debug routing; data it is allowed to mutate is tool-local UI/command history state and explicit debug command payloads routed through approved seams.

Placeholders (Physics, Audio, Scripting seam) owns reserved extension seams and interface placeholders that define where future systems integrate without changing current ownership boundaries; Placeholders (Physics, Audio, Scripting seam) must not own current vertical-slice runtime logic before those modules are implemented, and must not introduce hidden dependency reversals; Placeholders (Physics, Audio, Scripting seam) may call only seam-definition helpers and compile-time contracts needed to keep boundaries explicit; Placeholders (Physics, Audio, Scripting seam) must not call active gameplay mutation paths until implemented under dedicated tickets; data it is allowed to mutate is placeholder-local configuration/contract descriptors only.
### C. Seam invariants
- Loop owns timing and command routing.
- Scene owns gameplay rules.
- World owns runtime state and deferred mutation apply points.
- Renderer is read-only over world state and camera; no gameplay mutation.
- Content is compiled ahead of runtime; runtime must not parse XML.
- Save/load is validation-first; on failure, no partial world mutation occurs.
- Scene switching semantics remain as implemented (persistent worlds per scene; explicit hard reset path).
- Console commands that mutate gameplay route through a scene debug hook, not through loop runner directly.

## Ticket 33 Systems Seam (2026-02-21)
- `GameplayScene` now owns a `GameplaySystemsHost` lane in `crates/game/src/main.rs` and runs it exactly once per tick before a single post-host mutation apply point.
- Deterministic system order is explicit and fixed: `InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup` (ASCII-only).
- Systems receive `GameplaySystemContext` with `WorldView` (read-only wrapper over `SceneWorld`), tick input, and scene-local event/intent buffers.
- Systems do not mutate `SceneWorld` directly; world/entity mutations are applied only in `apply_gameplay_tick_at_safe_point`.
- Debug seam change: `DebugInfoSnapshot.system_order` is now owned `String` and overlay shows `sys: <order>`.

## Ticket 34 Event Bus Seam (2026-02-21)
- `GameplayEventBus` in `crates/game/src/main.rs` is scene-local and supports same-tick chaining via `emit` + `iter_emitted_so_far`.
- End-of-tick lifecycle uses rollover (`finish_tick_rollover`) to snapshot `last_tick_counts` and clear current tick events.
- Event set now includes payload-bearing variants for upcoming phases: `InteractionStarted`, `InteractionCompleted`, `EntityDamaged`, `EntityDied`, `StatusApplied`, `StatusExpired`.
- `DebugInfoSnapshot` gained generic `extra_debug_lines: Option<Vec<String>>` so game-specific observability can be surfaced without engine event-specific fields.
- Overlay inspect block renders any `extra_debug_lines`; gameplay uses this for `ev:` and `evk:` per-last-tick event counts.

## Ticket 35 Intent Apply Seam (2026-02-21)
- `GameplayIntentQueue` is scene-local and is the only mutation request path used by systems; intents are applied exactly once per tick in `apply_system_outputs` via `apply_gameplay_intents_at_safe_point`.
- Intent set is explicit and ordered: `SpawnByArchetypeId`, `DespawnEntity`, `ApplyDamage`, `AddStatus`, `RemoveStatus`, `StartInteraction`, `CompleteInteraction`.
- Runtime game-owned component stores live in `GameplayScene`: `health_by_entity: HashMap<EntityId, u32>` and `status_ids_by_entity: HashMap<EntityId, Vec<StatusId>>`.
- `StatusId` is a dedicated newtype (`StatusId(u64)`) to reduce churn when statuses become data-driven.
- Death rule is explicit: `ApplyDamage` only mutates health store; despawn requires explicit `DespawnEntity`.
- `CompleteInteraction` is mechanical-only for now (actor order state update); no resource grant/depletion logic is baked into this intent.
- Invalid intent targets are non-panicking: skipped and counted in `invalid_target_count`.
- Debug observability uses generic overlay extra lines: `in:`, `ink:`, `in_bad:` plus `spawned_entity_ids` in apply stats for deterministic tests/debug only.
- Same-tick spawned-entity reference handles are intentionally not supported in intent payloads yet.

## Ticket 36 Interaction Seam (2026-02-21)
- Interaction flow is now system-driven with scene-local runtime state: `active_interactions_by_actor` and monotonic `InteractionId`.
- `InputIntent` starts interactions only (`InteractionStarted` event + `StartInteraction` intent), and never emits completion.
- `Interaction` system owns all completion paths (including duration `0.0` immediate) and emits `InteractionCompleted` + `CompleteInteraction` intent only on successful completion.
- `GameplayIntent::CancelInteraction { actor_id }` was added; all cancellation paths use it and `CompleteInteraction` is reserved for successful completion only.
- Safe-point apply handles `StartInteraction`, `CancelInteraction`, and `CompleteInteraction` mechanically via actor order-state transitions; invalid targets remain non-panicking.
- Legacy direct completion/depletion logic was removed from `apply_gameplay_tick_at_safe_point`; world mutation outcomes now flow through intents only.
- Overlay debug block now includes interaction runtime line: `ix: ...`, and intent-kind counters include cancellation as `ca` in `ink: ...`.

## Ticket 37 AI Lite Seam (2026-02-21)
- `GameplaySystemId::AI` now runs real logic in `crates/game/src/main.rs` via scene-owned `ai_agents_by_entity: HashMap<EntityId, AiAgent>`.
- AI state machine MVP states are `Idle`, `Wander`, `Chase`, and `UseInteraction`; overlay extra lines include aggregate state counts as `ai: id:<n> wa:<n> ch:<n> use:<n>`.
- New intent `GameplayIntent::SetMoveTarget { actor_id, point }` was added and is applied only at the safe apply point; `ink:` now includes `mt:<n>`.
- AI interaction attacks use the existing interaction runtime seam by adding `ActiveInteractionKind::{Use,Attack}`. Attack completion/cancel still flows only through `Interaction` system events/intents.
- Movement precedence rule is enforced in AI: it never enqueues `SetMoveTarget` when actor has an active interaction runtime entry or world `OrderState::{Interact,Working}`.
- `player_id` is authoritative: `spawn proto.player` creates an AI-controlled actor but never replaces `GameplayScene.player_id`.
- Scene now auto-restores exactly one authoritative player when missing via `ensure_authoritative_player_exists_if_missing` during tick apply.

## Microticket 37.1 Content Expansion (2026-02-21)
- `assets/base/defs.xml` now includes `proto.npc_chaser`, `proto.npc_dummy`, `proto.stockpile_small`, and `proto.door_dummy` in addition to existing defs.
- Spawn runtime role wiring in `crates/game/src/main.rs` is now tag-driven for debug/intents: `actor` tag spawns with `spawn_actor`, `interactable` tag initializes runtime `Interactable` data.

## Microticket 46.3 Thruport Delivery + Status (2026-02-21)
- Added queueable console command `thruport.status` in engine command registry.
- `thruport.status` output contract is exact and unprefixed: `thruport.status v1 enabled:<0|1> telemetry:<0|1> clients:<u32>`.
- `RemoteConsoleLinePump` now exposes `status_line(telemetry_enabled)`; engine uses this to append status through the same console output path used for remote readback.
- Game-side thruport (`crates/game/src/app/dev_thruport.rs`) now sets accepted client sockets to `TCP_NODELAY` to reduce telemetry line coalescing latency.
- Input injection command acknowledgements remain deterministic (`ok: injected input.<...>`) and are verified through the same remote output forwarding seam.
- `proto.player` now includes the `actor` tag to preserve tag-driven spawn behavior compatibility.
- `proto.door_dummy` currently carries `immediate_use` as metadata only; immediate duration behavior is explicitly deferred (no interaction logic changes in this microticket).

## Ticket 38 Combat Lite Seam (2026-02-21)
- Combat is now resolved in `GameplaySystemId::CombatResolution` by deriving `ApplyDamage` intents only from `InteractionCompleted` events proven to come from `ActiveInteractionKind::Attack` completions in the same tick.
- Scene runtime health store in `crates/game/src/main.rs` is now `HashMap<EntityId, Health>` with `Health { current, max }`; health is initialized for actors only.
- Safe-point intent apply now uses a same-pass pending queue, so follow-up `DespawnEntity` generated by death during `ApplyDamage` is processed deterministically in the same apply invocation.
- `ApplyDamage` behavior:
  - if target missing: invalid target counter increments, no panic
  - if target lacks health: ignored, debug breadcrumb logged, invalid target counter increments
  - if damage applies: emits `EntityDamaged { entity_id, amount_applied }`
  - on transition to zero health: emits `EntityDied { entity_id }` and enqueues same-pass `DespawnEntity`
- Dev probe no longer emits synthetic `EntityDamaged`/`EntityDied`, so `evk dm/dd` now reflects real combat outcomes.

## Ticket 39 Status Effects Seam (2026-02-21)
- Status ids are now string ids (`StatusId(&'static str)`), with shipping status `status.slow`.
- Runtime status storage is scene-owned timed sets: `status_sets_by_entity: HashMap<EntityId, StatusSet>`, where each `StatusSet` contains `ActiveStatus { status_id, remaining_seconds }`.
- `GameplayIntent::AddStatus` now carries `duration_seconds`; reapply uses a single rule: refresh remaining duration.
- `StatusApplied` is emitted only when a status is newly added or refreshed; `StatusExpired` is emitted only when a present status is actually removed.
- `GameplaySystemId::StatusEffects` now runs real ticking logic, decrements durations deterministically, and enqueues `RemoveStatus` intents for expirations.
- Movement modifier is derived, not directly mutated: effective speed uses the product of active status multipliers (`status.slow` currently multiplies by `0.5`; unknown statuses are neutral `1.0`).
- Combat integration now applies `status.slow` on attack-completion-derived damage path (`CombatResolution`), preserving attack-only guard.


## Ticket 40 Main Modularization (2026-02-21)
- `crates/game/src/main.rs` is now a thin composition root (`mod app;`, `build_app`, `run`, `ExitCode`) and is under 300 lines.
- Startup wiring moved to `crates/game/src/app/bootstrap.rs`:
  - owns tracing init, `PROTOGE_ENABLED_MODS` parsing, scene pair construction call, and `LoopConfig` assembly.
  - contract: `build_app() -> AppWiring { config, scene_a, scene_b }`.
- Engine loop invocation moved to `crates/game/src/app/loop_runner.rs`:
  - contract: `run(AppWiring) -> ExitCode`.
  - preserves startup error logging (`startup_failed`) and non-zero exit behavior.
- Gameplay/runtime implementation and tests moved from `crates/game/src/main.rs` into `crates/game/src/app/gameplay.rs` with no behavioral intent changes.
- Gameplay scene factory seam added for bootstrap usage: `build_scene_pair() -> (Box<dyn Scene>, Box<dyn Scene>)`.

## Ticket 41 Dev Thruport Seam (2026-02-21)
- Added `crates/game/src/app/dev_thruport.rs` as a compile-only no-op seam for future thruport/remote console work.
- Defined forward-looking hook contracts only:
  - `ConsoleInputQueueHook::drain_pending_lines`
  - `ConsoleOutputTeeHook::tee_output_line`
  - `InputInjectionHook::inject_input`
- Added placeholder input payload enum `InjectedInput` (`NoOp`, `KeyDown`, `KeyUp`, `MouseMove`).
- Added allocation-free no-op wiring types:
  - `DevThruport`
  - `DevThruportHooks::no_op()`
  - `initialize(DevThruportHooks) -> DevThruport`
- Bootstrap now initializes and carries `AppWiring.dev_thruport`; loop runner explicitly destructures and binds it without affecting runtime behavior.
- No TCP/network/screenshot/input synthesis behavior added; console semantics are unchanged.

## Ticket 42 TCP Thruport Transport (2026-02-21)
- Added localhost-only TCP remote console transport in `crates/game/src/app/dev_thruport.rs`.
- Enablement is env-driven:
  - `PROTOGE_THRUPORT=1` enables listener
  - `PROTOGE_THRUPORT_PORT=<u16>` sets port (default `46001`)
- Bind target is hard-locked to `127.0.0.1:<port>`.
- Transport is non-blocking and line-based (`\n` delimited UTF-8, CRLF tolerated via trailing `\r` strip).
- Invalid UTF-8 lines are dropped with a warning; client/socket errors are non-fatal and do not block the frame loop.
- Added minimal engine runtime hook seam to feed remote lines into the existing console pending-lines path:
  - `RemoteConsoleLinePump`
  - `LoopRuntimeHooks`
  - `run_app_with_hooks(...)`
- Engine redraw flow now polls remote lines once before `ConsoleCommandProcessor::process_pending_lines`, and enqueues via `ConsoleState::enqueue_pending_line`.
- When disabled, no socket is opened and behavior remains equivalent to prior path.

## Ticket 43 Console Output Tee + Remote Readback (2026-02-21)
- Added bounded console output delta buffer in engine console state to expose "new output since last poll".
- `ConsoleState::append_output_line` now tees each output line into:
  - existing visible output history
  - bounded remote-drain buffer (`MAX_NEW_OUTPUT_LINES`).
- Added `ConsoleState::drain_new_output_lines_into(...)` and ensured `clear_output_lines()` clears both output stores.
- Extended engine thruport hook trait `RemoteConsoleLinePump` with default no-op `send_output_lines(&[String])`.
- Engine redraw path now forwards newly appended console output lines to remote hook after command processing/execution.
- Game thruport transport now writes newline-delimited UTF-8 output lines to connected TCP clients via non-blocking writes.
- Readback remains localhost-only transport behavior and does not alter console command semantics or output text.

## Ticket 44 Input Injection Bridge (2026-02-21)
- Added engine queueable debug command family `input.*` (`input.key_down`, `input.key_up`, `input.mouse_move`, `input.mouse_down`, `input.mouse_up`) in `crates/engine/src/app/tools/console_commands.rs`.
- `DebugCommand` now carries `InjectInput { event }` with explicit injected payload types (`InjectedInputEvent`, `InjectedKey`, `InjectedMouseButton`) and strict parse validation/usage errors.
- Input injection apply seam is in `crates/engine/src/app/loop_runner.rs`:
  - commands enqueue injected events into `InputCollector`
  - injected event queue drains only at `InputCollector::snapshot_for_tick` (single stable point per tick)
  - injected action/button states are merged into normal `InputSnapshot` for gameplay reads.
- Console-open suppression remains intact: when console is open, gameplay-facing snapshot is still cleared even if injected events are queued.
- Added disconnect safety reset seam:
  - `RemoteConsoleLinePump::take_disconnect_reset_requested()` default no-op method
  - thruport implementation raises one-shot reset when connected remote clients drop to zero
  - loop marks collector reset and clears held injected keys/buttons on the next snapshot tick.

## Ticket 45 Deterministic Step Control (2026-02-21)
- Added queueable engine simulation-control commands in `crates/engine/src/app/tools/console_commands.rs`:
  - `pause_sim`
  - `resume_sim`
  - `tick <steps>` (`u32`, `> 0`)
- Loop runner in `crates/engine/src/app/loop_runner.rs` now owns sim gating state:
  - `sim_paused: bool`
  - `queued_manual_ticks: u32`
- While paused, frame-time accumulation is disabled and only queued manual ticks advance simulation; rendering/frame pacing still run normally.
- Manual ticks execute through the exact same fixed update path (`snapshot_for_tick` + `scenes.update_active` + apply/switch handling), preserving deterministic behavior and avoiding alternate loops.

## Ticket 46 State Probe Commands (2026-02-21)
- Added queueable console commands `dump.state` and `dump.ai` in `crates/engine/src/app/tools/console_commands.rs`.
- Extended scene debug command routing (`SceneDebugCommand::{DumpState, DumpAi}`) so probe output is scene-owned and goes through existing `execute_debug_command` seam.
- `GameplayScene` now emits versioned, single-line deterministic probe payloads:
  - `dump.state v1 | ...` with player/camera/selection/target/entity/event/intent counters.
  - `dump.ai v1 | ...` with AI state counts and optional deterministic top-5 nearest agent list.
- Probe formatting rules:
  - fixed field order
  - fixed float precision (`{:.2}`)
  - no pointer/internal unstable data beyond existing entity IDs.

## Microticket 46.1 Thruport Frame Telemetry (2026-02-21)
- Added opt-in thruport telemetry push line behind `PROTOGE_THRUPORT_TELEMETRY=1` (requires thruport enabled).
- Telemetry schema (single UTF-8 newline-delimited line, no JSON):  
  `thruport.frame v1 tick:<u64> paused:<0|1> qtick:<u32> ev:<u32> in:<u32> in_bad:<u32>`
- Emission point is engine loop fixed-step execution: exactly one telemetry line per executed fixed tick on the normal update path (includes manual `tick <N>` execution path).
- `tick` is monotonic loop-owned counter; `paused`/`qtick` come from existing sim gating state; `ev`/`in`/`in_bad` are derived from active-scene debug snapshot extra lines (`ev:`, `in:`, `in_bad:`), with zero fallback when unavailable.
- Existing console `ok:`/`error:` output lines remain unchanged and unprefixed.

## Microticket 46.2 Sync Barrier + Reset Semantics (2026-02-21)
- Added queueable engine console command `sync` that appends exactly `ok: sync` in queued command order, acting as a command-processing barrier for remote automation transcripts.
- Tightened thruport disconnect reset semantics in game transport: one-shot reset flag is now set whenever any TCP client is removed (EOF/read error/write error), not only when all clients disconnect.
- Hardened `dump.state v1` format contract tests in gameplay:
  - fixed key order checks (`player -> cam -> sel -> tgt -> cnt -> ev -> evk -> in -> ink -> in_bad`)
  - required-key presence checks for populated and empty-world cases
  - fixed two-decimal precision assertions for player/camera coordinates.

## Microticket 46.4 A1/E1 Barrier Audit + Harness Fix (2026-02-21)
- Added opt-in thruport diagnostics behind `PROTOGE_THRUPORT_DIAG=1` in:
  - `crates/game/src/app/dev_thruport.rs` (remote line read, control/telemetry enqueue, flush progress/would-block/completion)
  - `crates/engine/src/app/loop_runner.rs` (remote lines polled, queueable command execution tokens, output lines forwarded to remote)
- Added engine guard test in `crates/engine/src/app/loop_runner.rs`:
  - `reset_pause_sync_commands_emit_ordered_ok_lines`
  - asserts queueable burst order and output: `ok: scene reset`, `ok: sim paused`, `ok: sync`.
- Root cause found for A1/E1 false negatives was harness-side read logic (DataAvailable-gated transcript capture missing buffered `StreamReader` lines), not command execution/output routing.
- Recreated canonical harness scripts in `.codex_artifacts/`:
  - `run_minset_telemetry.ps1` (reader-safe barrier loop; A1/E1 gate)
  - `run_minset_simple.ps1` (delegates to telemetry script)
  - `SOME_COMMANDS.md` restored as baseline manual sequence.

## Ticket 47 Deterministic Select + Order Commands (2026-02-21)
- Added queueable engine commands for deterministic automation without pixel input: `select <entity_id>`, `order.move <x> <y>`, `order.interact <target_entity_id>`.
- Extended engine/game seam enums:
  - `DebugCommand::{Select, OrderMove, OrderInteract}` in `crates/engine/src/app/tools/console_commands.rs`.
  - `SceneDebugCommand::{Select, OrderMove, OrderInteract}` in `crates/engine/src/app/scene.rs`.
- `GameplayScene::execute_debug_command` in `crates/game/src/app/gameplay.rs` now supports:
  - direct selectable-entity selection (`select`)
  - move intent enqueue via `GameplayIntent::SetMoveTarget` (`order.move`)
  - interaction intent/event enqueue via existing interaction runtime seam (`order.interact`)
- Failure contract is explicit and non-panicking: no selection, missing/stale selected entity, non-actor selected entity, missing/non-interactable target.

## Microticket 46.5 Injected Cursor Reliability Fix (2026-02-21)
- Root cause for remote right-click no-op in hidden-window thruport runs: native `CursorLeft` events could clear `cursor_position_px` between injected mouse commands and tick snapshot.
- `InputCollector` in `crates/engine/src/app/loop_runner.rs` now tracks `injected_cursor_position_px` separately from native cursor state.
- Tick snapshots now use merged cursor priority `injected_cursor_position_px` then native `cursor_position_px`, so injected `input.mouse_move` remains deterministic for automation clicks.
- Disconnect/input reset paths clear injected cursor state to avoid stale coordinates across sessions.

---

## Migration Notes (2026-02-28)
- Moved deprecated in-place detailed notes from `CODEXNOTES.md` into archive to keep living context concise per `AGENTS.md`.
- Scope moved: module-boundary details, Ticket 48-54 detailed bullets, and single-shot stress test update notes.

## Module Boundaries and Ownership (Moved from CODEXNOTES.md, Deprecated In-Place on 2026-02-28)
### A. Module map
#### Core
- Core primitives and IDs shared across engine/game crates.
#### App/Loop
- Frame loop, fixed timestep cadence, runtime metrics, and scene tick orchestration.
#### SceneMachine and Scene
- Active/inactive scene ownership, scene switching, load/reset boundaries.
#### World (SceneWorld and runtime state)
- Entity storage, transforms, runtime tags/components, spawn/despawn application.
#### Rendering
- Camera transforms, world-to-screen math, sprite/placeholder draw path.
#### Assets and Content Pipeline
- XML discovery/compile, cached content packs, runtime `DefDatabase`.
#### Input
- Per-frame input sampling and action mapping.
#### Tools (Overlay, Console)
- Debug overlay text, console command routing, diagnostics surfaces.
#### Placeholders (Physics, Audio, Scripting seam)
- Reserved seams only; no heavy implementation yet.
### B. Ownership rules
- Engine owns generic runtime/data flow; game crate owns gameplay rules/defs consumption.
- Runtime sim does not parse XML; content is consumed via compiled `DefDatabase`.
- Keep systems deterministic and avoid broad cross-module coupling.
### C. Seam invariants
- Scene/game logic may read defs but must not mutate content database contracts.
- Def defaults for Ticket 48 gameplay knobs are centralized in `GameplayScene` helper.
- Simulation intent ordering remains `InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup`.

## Ticket Notes (2026-02-23, moved from CODEXNOTES.md on 2026-02-28)
- Ticket 48 (2026-02-23): `EntityDef` now carries optional gameplay knobs in content runtime data (`health_max`, `base_damage`, `aggro_radius`, `attack_range`, `attack_cooldown_seconds`) through compile/pack/database/archetype as `Option`.
- Ticket 48: gameplay runtime defaults for those knobs are centralized in `GameplayScene::effective_combat_ai_params` in `crates/game/src/app/gameplay.rs` to preserve legacy behavior when fields are omitted.
- Ticket 48: attacker damage source is `GameplayScene.damage_by_entity: HashMap<EntityId, u32>`; populated during `SpawnByArchetypeId`, consumed by `CombatResolution`, and cleaned on sync/reset/despawn.
- Ticket 49 (2026-02-23): `ContentCompileError` now includes optional structured context fields `def_name` and `field_name` so gameplay tuning validation failures are deterministic and testable.
- Ticket 49: gameplay tuning validation fixture added at `docs/fixtures/content_pipeline_v1/fail_09_invalid_gameplay_field/badgameplay/defs.xml`; pipeline tests assert `InvalidValue` plus `mod_id`/`def_name`/`field_name`.
- Ticket 50 (2026-02-23): regression coverage for knobbed sample content is anchored in `crates/engine/src/content/pipeline.rs` test `base_defs_load_proto_npc_chaser_with_expected_tuning_fields` (loads base defs and asserts parsed tuning values for `proto.npc_chaser`).
- Ticket 50: gameplay smoke coverage for tuned chaser + shipping slow lifecycle is in `crates/game/src/app/gameplay.rs` test `proto_npc_chaser_attack_applies_slow_then_slow_expires`.
- Ticket 51.1 (2026-02-23): queueable command `thruport.telemetry <on|off>` added in `crates/engine/src/app/tools/console_commands.rs`; runtime loop telemetry state is mutable per-session with explicit schema output `ok: thruport.telemetry v1 enabled:<0|1>`.
- Ticket 51.1: remote TCP wire contract now applies last-mile channel prefixes in `crates/game/src/app/dev_thruport.rs` (`C ` control, `T ` telemetry), and sends per-client ready line on accept: `C thruport.ready v1 port:<u16>`.
- Ticket 51.2 (2026-02-23): repo-owned CLI harness added as workspace crate `crates/thruport_cli` (`cargo build -p thruport_cli`) for deterministic thruport automation without shell socket scripts.
- Ticket 51.2: CLI contract lives in `crates/thruport_cli/src/lib.rs` and `crates/thruport_cli/src/main.rs` with subcommands `wait-ready`, `send`, `script [--barrier]`, `barrier`; default output is control-only and strips `C/T` tags, `--include-telemetry` includes telemetry payloads.
- Ticket 51.3 (2026-02-23): queueable command `scenario.setup <scenario_id>` added to engine parser/routing (`crates/engine/src/app/tools/console_commands.rs`, `crates/engine/src/app/loop_runner.rs`) and scene debug API (`crates/engine/src/app/scene.rs`).
- Ticket 51.3: gameplay scene owns `combat_chaser` setup in `crates/game/src/app/gameplay.rs` via safe-point intents only; contract line is `ok: scenario.setup combat_chaser player:<id> chaser:<id> dummy:<id>`.
- Ticket 51.3: deterministic overwrite rule is scene-local slot replacement (previous scenario chaser/dummy and current authoritative player are despawned, then player/chaser/dummy respawn at fixed coordinates and player is re-selected).
- Ticket 51.4 (2026-02-23): added repo tooling helper `scripts/test-helper.ps1` for deterministic test discovery and single-test execution.
- Ticket 51.4: helper supports `-Mode list` and `-Mode run-one` for packages `engine|game|thruport_cli`; `run-one` resolves regex to exactly one canonical test name and executes with `cargo test -p <pkg> <name> -- --exact`.
- Ticket 51.4: workflow docs added in `docs/test_helper.md` and linked from `README.md`.
- Ticket 52 (2026-02-23): `crates/game/src/app/gameplay.rs` was structurally decomposed into `crates/game/src/app/gameplay/` chunk files (`mod.rs`, `types.rs`, `systems.rs`, `scene_state.rs`, `scene_impl.rs`, `util.rs`, `tests.rs`) using include-based composition to preserve runtime behavior and private-access semantics.
- Ticket 52: external gameplay entrypoint contract is unchanged (`crate::app::gameplay::build_scene_pair`), and system order constants/intent pipeline behavior remain intact.
- Ticket 53 (2026-02-23): `thruport_cli send` now uses an internal `sync` completion boundary in `crates/thruport_cli/src/lib.rs` and suppresses the internal `ok: sync` line from default output.
- Ticket 53: `thruport_cli` fallback behavior uses quiet-window completion only when internal sync is unavailable; CLI now exposes `--quiet-ms` (default `250`) in `crates/thruport_cli/src/main.rs` and `docs/thruport_cli.md`.
- Ticket 53: `script --barrier` contract remains one explicit end-of-script barrier (no added per-command barriers).
- Ticket 54 (2026-02-23): thruport telemetry emission in `crates/engine/src/app/loop_runner.rs` is now driven by executed fixed ticks via `emit_thruport_tick_telemetry_if_enabled`, covering both accumulator-driven and paused manual queued ticks without changing the `thruport.frame v1` schema or channel tagging.
- Ticket 54: engine coverage added in `crates/engine/src/app/loop_runner.rs` test `telemetry_emits_for_manual_ticks_while_paused_when_enabled` to assert manual tick execution emits non-zero telemetry frames when enabled.

## Single-shot stress test doc update (2026-02-23, moved from CODEXNOTES.md on 2026-02-28)
- Preconditions/reporting update: `cargo fmt --all -- --check` is non-gating and should be reported as `WARN` when it fails (do not fail the single-shot run on fmt alone).
- Subtest 08 (revised deterministic script path):
  1) Create `tick_telemetry_probe.txt` with lines:
     - `pause_sim`
     - `thruport.telemetry on`
     - `tick 5`
     - `sync`
  2) Run:
     - `thruport_cli --port 46001 --include-telemetry --quiet-ms 1000 script .\tick_telemetry_probe.txt --barrier`
  3) PASS criteria:
     - Control acks include `ok: sim paused`, `ok: thruport.telemetry v1 enabled:1`, `ok: queued tick 5`, and `ok: sync`.
     - Telemetry output includes exactly 5 lines matching `thruport.frame v1 ... paused:1 ...` for the queued manual ticks.

---

## Migration Notes (2026-03-01)
- Moved detailed historical notes from `CODEXNOTES.md` into archive to keep `CODEXNOTES.md` as active-context only.
- Scope moved: status model reminder, Tickets 55-65 detail bullets, and legacy module-boundary dump.

## Ticket Notes (Moved from CODEXNOTES.md on 2026-03-01)
- Status model reminder: statuses use `StatusId(&'static str)` and shipping slow status id is `status.slow`.
- Ticket 55 (2026-02-28): renderer now owns a micro-grid snap policy for world-space draw placement (`crates/engine/src/app/rendering/renderer.rs`), defaulting to `MICRO_GRID_RESOLUTION_PX = 1`; simulation transforms and picking logic remain unchanged.
- Ticket 56 (2026-02-28): added floor-layer runtime contract with `FloorId` (Rooftop/Main/Basement), `Entity.floor`, and `SceneWorld.active_floor`; topmost pick functions now take optional floor filters (`None` keeps legacy behavior) and `floor.set` routes as an engine queueable command to scene-owned behavior.
- Ticket 57 (2026-02-28): gameplay orderability contract now restricts job/order commands (`order.move`, `order.interact`, right-click job intents) to the authoritative `player_id`; non-player NPC actors remain selectable but are non-jobbable, and combat AI auto-registration now requires archetype combat fields instead of applying to every non-player actor.
- Ticket 57 (2026-02-28, occlusion assist v0): renderer affordances now support deterministic occlusion assist using renderer overlap order keys (`Entity::renderer_overlap_order_key`) and placeholder-sized screen bounds; `SceneVisualState` gained `targeted_interactable`, gameplay populates it from selected actor `Interact/Working` target on active floor, and renderer draws x-ray outline passes without changing simulation or picking.
- Ticket 58.1 (2026-02-28): added tiny visual-test sprite assets under `assets/base/sprites/visual_test/` and rewired `proto.player`, `proto.npc_chaser`, and `proto.npc_dummy` sprite keys to those assets; renderer sprite-load fallback remains unchanged, with new rate-limited once-per-key warning logs for missing/invalid/decode-failed sprite loads.
- Ticket 58.2 (2026-02-28): added gameplay scenario id `visual_sandbox` to `scenario.setup`, which deterministically clears live entities then spawns `proto.player`, `proto.resource_pile`, `proto.door_dummy`, and `proto.stockpile_small` at fixed on-screen coordinates with an intentional overlap at `(0,0)`; success payload schema is `scenario.setup visual_sandbox player:<id> prop:<id> wall:<id> floor:<id>`.
- Ticket 59 (2026-02-28): sprite renderables now support optional XML `pixelScale` (1..=16, default 1) carried through runtime and content pack (`CONTENT_PACK_FORMAT_VERSION` bumped to 3); renderer applies integer nearest-neighbor sprite enlargement via scaled draw dimensions while keeping pixel-snap placement and all gameplay/picking semantics unchanged.
- Ticket 60 (2026-02-28): save/load v3 remains backward-compatible but now carries optional floor/archetype identity fields (`SaveGame.active_floor`, `SavedEntityRuntime.floor`, `SavedEntityRuntime.archetype_def_name`); apply-save restores per-entity floors by temporarily setting `SceneWorld.active_floor` before each spawn and then reapplies saved scene active floor. Gameplay now tracks `entity_archetype_id_by_entity` and reuses `target_lookup_by_save_id` across ticks to avoid per-tick map reallocations, while restore/rebuild paths derive combat defaults from persisted archetype identity when available.
- Ticket 60 (2026-02-28, rendering/transport/content robustness): renderer world pass now consumes an explicit sorted visible draw list (`renderer_overlap_order_key` + `EntityId` tiebreak) instead of relying on entity storage order, and `draw_sprite_centered_scaled` now clips destination bounds once and uses precomputed inverse-scale row/column mapping. Content atomic writes now use a std-only backup-rename replacement flow (no delete gap), and thruport now enforces a per-client control-queue byte cap with oldest-control eviction while preserving control-before-telemetry queue ordering.
- Ticket 61.1 (2026-02-28): added dev-only Command Palette UI in engine tools (`crates/engine/src/app/tools/command_palette.rs`) that emits command strings into existing console pending-line parsing flow (no parser/routing semantics change). Palette supports immediate buttons plus single-use armed spawn placement (`spawn <def> x y`) with world-space click resolution, right-click cancel, and panel-hit exclusion from placement; spawn presets are sourced from active-world `DefDatabase` def names.
- Ticket 61.2 (2026-02-28): debug overlay readability pass updated `crates/engine/src/app/tools/overlay.rs` with a high-contrast text palette, deterministic grouped spacing, and an always-on solid alpha backing plate + border behind overlay text; no metrics/simulation/input/gameplay behavior changed.
- Ticket 62.1 (2026-02-28): added visual-only per-entity action payload contract (`ActionState`, `ActionParams`, `CardinalFacing`, `ActionTargetHint`, `EntityActionVisual`) under `SceneWorld.visual_state.entity_action_visuals` keyed by `EntityId`; missing entries resolve to `Idle` + zero params.
- Ticket 62.1 (2026-02-28): gameplay now emits player `Idle`/`Walk` + cardinal facing every tick via `SceneWorld::set_entity_action_visual`, with gameplay-owned `last_player_facing` persistence; renderer remains stateless for facing fallback and must not use this payload for simulation decisions.
- Ticket 62.2 (2026-02-28): sprite renderables now support optional fixed-name anchors stored as integer pixels (`SpriteAnchors` with `SpriteAnchorPx{i16}`), authored via XML `<anchors><anchor .../></anchors>` and persisted through content pack format v4.
- Ticket 62.2 (2026-02-28): renderer anchor transform policy is West-only horizontal mirror (`x_px -> -x_px`); East/North/South apply no runtime anchor transform and rely on distinct authored sprite art for orientation differences.
- Ticket 62.2 (2026-02-28): carry attachment remains visual-only: when action visual is `Carry` with `held_visual`, renderer resolves the def and draws its sprite at the entity carry anchor (missing anchor/def safely falls back or skips without per-frame log spam).
- Ticket 62.3 (2026-02-28): renderer now applies a visual-only procedural offset layer for `Idle`/`Walk`/`Hit` derived from the fixed-step loop tick counter (not frame-dt), so motion remains deterministic across FPS and does not mutate simulation transforms/picking state.
- Ticket 62.3 (2026-02-28): procedural offsets are applied before the existing micro-grid snap path for both base entity draw and carry attachment draw; `rotation_radians` is computed internally but intentionally unused by current sprite draw routines.
- Ticket 62.4 (2026-02-28): renderer sprite-variant fallback is scoped to `visual_test/` keys only, with deterministic lookup order `{base}__{state}_{facing}` -> `{base}__{state}` -> `{base}`; non-`visual_test/` keys bypass variant probing entirely.
- Ticket 62.4 (2026-02-28): `visual_sandbox` keeps the role-stable success payload schema (`player/prop/wall/floor`) and role semantics, while additional demo interactables may spawn outside payload for deterministic showcase coverage.
- Ticket 62.4 (2026-02-28): sandbox-only player action forcing uses deterministic rules (interaction target tags -> `Interact`/`UseTool`, carry-lane x-threshold -> `Carry`, otherwise movement `Idle`/`Walk`) with no progression/unlock state.
- Ticket 63 (2026-02-28): gameplay now tracks `pawn_role_by_entity` for actor control roles (`PlayerPawn`, `Settler`, `Npc`), with role entries populated only when spawn intents are applied and a committed live `EntityId` exists.
- Ticket 63 (2026-02-28): `PlayerPawn` role assignment is authority-owned only (`GameplayScene.player_id`), never inferred from `defName`; `spawn proto.player` remains non-authoritative by default.
- Ticket 63 (2026-02-28): orderability gates for right-click and `order.move` / `order.interact` now target selected orderable pawns (`PlayerPawn` + `Settler`) while NPC actors stay selectable but non-orderable; keyboard movement remains authoritative-player-only.
- Ticket 63 (2026-02-28): `scenario.setup visual_sandbox` now also spawns one deterministic settler (`proto.settler`) without changing the success payload schema (`player/prop/wall/floor`).
- Ticket 64 (2026-03-01): Settler navigation is gameplay-owned (`crates/game/src/app/gameplay/nav.rs`) with deterministic tile A* (4-neighbor `N,E,S,W`) and deterministic open-list tie-break ordering `(f,h,y,x,insertion_order)` using a monotonic insertion counter.
- Ticket 64 (2026-03-01): nav cache invalidation uses deterministic `TilemapNavKey { width, height, origin bits, tiles_hash }`; `tiles_hash` is stable FNV-1a over tile IDs (no pointer/address identity).
- Ticket 64 (2026-03-01): Settler `order.move` snaps `goal_tile = world_to_tile(target_world)` and completes at the goal tile center; unreachable/blocked/out-of-bounds goals fail deterministically and settle to `Idle`.
- Ticket 64 (2026-03-01): added deterministic scenario `scenario.setup nav_sandbox` (`player:<id> settler:<id>`) with a blocked strip (tile id `2`) that forces detour pathing.
- Ticket 65 (2026-03-01): gameplay now has a first-class runtime `JobBoard` (`JobId`, `JobKind`, `JobTarget`, `JobState`, reservation + assignment map) and Settler runner phases (`Idle`, `Navigating`, `Interacting`).
- Ticket 65 (2026-03-01): assigning a new job to a Settler deterministically interrupts same-tick prior assignment by failing old job, clearing nav/`OrderState`, and canceling active interaction before new assignment.
- Ticket 65 (2026-03-01): `OrderState` is now an actuator for Settler jobs; `OrderState::MoveTo` and nav path state must not outlive assigned job lifecycle (completion/failure/interruption clear both assignment and locomotion state atomically).
- Ticket 65 (2026-03-01): Settler `UseInteractable` jobs complete only from existing `InteractionCompleted` events; job runner never directly mutates target completion/resource outcomes.

## Module Boundaries and Ownership (Legacy Snapshot moved 2026-03-01)
### A. Module map
- Core: shared IDs, value types, and cross-module contracts.
- App/Loop: main loop, window/input pump, scene routing, and queueable command execution.
- SceneMachine and Scene: scene lifecycle (`load/update/render/unload`) and debug-command seam.
- World (SceneWorld and runtime state): runtime entities, camera, tilemap, visuals, debug markers, and pick helpers.
- Rendering: projection, world pass draw policy, sprite/tile draw, and overlay/console composition.
- Assets and Content Pipeline: XML discovery/compile, cache planning, and DefDatabase runtime load path.
- Input: action snapshots and edge-trigger semantics for simulation-safe input use.
- Tools (Overlay, Console): in-game console parsing/queueing and debug overlay text/perf presentation.
- Placeholders (Physics, Audio, Scripting seam): reserved seams only; no advanced subsystem ownership yet.
### B. Ownership rules
- Engine owns render policy, command parsing/routing, and scene machine orchestration.
- Game owns gameplay rules/state transitions and scene debug command behavior.
- Runtime simulation state mutates in gameplay safe points, not in rendering paths.
### C. Seam invariants
- Dependency direction remains engine -> game boundary-safe with scene trait seam.
- Rendering/picking policies may read runtime state but must not mutate simulation transforms.
- Console queueable outputs remain standardized as `ok:` or `error:` lines.
