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
