# CODEXNOTES.md

## Purpose
Living, structured notes for Codex and humans to keep consistent context across threads.
Keep this concise and actionable. Prefer bullet points. Avoid long code dumps.

---

## Decisions (Locked)
- Language/runtime: Rust
- Content authoring: XML
- Content runtime: compiled binary ContentPack
- Compile behavior: compile-on-first-launch, cache per mod, rebuild on change or invalid cache
- Simulation: fixed timestep (TPS), deterministic mindset
- Vertical slice priorities: boot → scene → entity move → render → debug overlay → clean quit

---

## Current Milestone
- Milestone: Vertical Slice v0
- Definition: window + loop + scene + controllable entity + render + overlay + clean quit

### Next Tickets (Queue)
- Ticket 0: Repo skeleton and one-command run
- Ticket 1: App loop lifecycle
- Ticket 2: Scene boundary and minimal entity model
- Ticket 3: Input mapping and controllable entity
- Ticket 4: Minimal rendering and camera seam
- Ticket 5: Tools overlay
- Ticket 6: Content pipeline contract
- Ticket 7: Mod discovery + compile plan
- Ticket 8: XML MVP compiler → DefDatabase
- Ticket 9: ContentPack v1 cache load/save
- Ticket 10: Override rule MVP

---

## Module Map (Ownership)
- Core: common types, IDs, error/reporting primitives
- App/Loop: window init, main loop, fixed timestep, lifecycle
- Scene: scene lifecycle, entity storage, spawn/despawn, world state container
- Rendering: camera, renderables, world-to-screen
- Input: action mapping, per-frame input sampling
- Assets: (placeholder) asset handles and paths, later async loading
- Tools: debug overlay, counters, diagnostics
- Content: (later) mod scan, XML compile, pack IO, DefDatabase

---

## Data Contracts (Plain English)

### Scene and Entity (runtime)
- Entity: stable ID + Transform + Renderable descriptor (placeholder)
- Transform: position (2D for now), optional rotation later
- Scene API: load / update(fixed_dt) / render / unload
- Scene owns entity list and spawn/despawn rules

### Content and Mods
- Mod: folder with XML files (and optionally art assets later)
- Load order: base content first, then mods in configured order
- Override rule MVP: last one wins for scalar fields

### DefDatabase (runtime)
- Stores compiled “Defs” (archetypes only, no runtime state)
- Uses numeric IDs internally (no string lookups in hot paths)
- Precomputed indices allowed (by tag/category/etc) when needed

### ContentPack v1 (cached binary)
- Per-mod cache file
- Includes: pack version, compiler version, input hash, deterministic ordering rules
- Includes mapping back to source file info for errors/debugging (minimal)

### Cache Key (invalidation)
- Inputs to cache validity:
  - Game version
  - Compiler version
  - Mod load order position / enabled mod list hash
  - Hash of all XML files in the mod (paths + contents)

---

## Performance Rules of Thumb
- Avoid per-tick allocations in simulation loop
- Avoid scanning all entities each tick; prefer caches/indices and time-slicing
- Separate sim update cadence from render cadence
- Do not introduce multithreading into simulation until profiling proves need
- Treat content parsing as load-time only; runtime never touches XML

---

## Logging and Error Model
- Errors should be actionable: mod name, file path, line/field if possible, and a clear message
- Content errors fail fast at load (don’t limp into runtime with partial state)
- Keep logs structured and minimal in hot paths

---

## Pitfalls and Gotchas (Update as found)
- (empty)

---

## Known Issues / TODO
- (empty)

---

## Change Log (Optional)
- YYYY-MM-DD: short note of major decisions or interface changes

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
