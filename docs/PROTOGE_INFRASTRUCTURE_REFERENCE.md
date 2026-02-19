# PROTOGE Infrastructure Reference

## 1) Project snapshot
- Proto GE is a prototype-first Rust game engine plus colony-sim vertical slice scaffold.
- Current status is Vertical Slice v0: window + loop + scene + controllable entity + rendering + overlay + clean quit.
- Ticket outcomes (0-10):
  - Ticket 0: workspace bootstrap, canonical directories, deterministic root discovery.
  - Ticket 1: fixed-timestep loop, responsive window lifecycle, structured loop metrics.
  - Ticket 2: scene lifecycle seam, scene switching, engine-owned entity/world model.
  - Ticket 3: action-based input seam and controllable entity movement.
  - Ticket 4: `pixels` rendering path and camera/world-to-screen seam.
  - Ticket 5: always-available tools overlay with safe clipping text blitter.
  - Ticket 6: content pipeline contract spec (`docs/content_pipeline_contract_v1.md`).
  - Ticket 7: deterministic mod discovery and compile-plan rails.
  - Ticket 8: strict XML `EntityDef` compiler to runtime `DefDatabase`.
  - Ticket 9: binary content pack cache with atomic writes and safe fallback.
  - Ticket 10: field-level override merge with deterministic conflict behavior.

## 2) Repository map
- Primary folders:
  - `crates/engine`: engine runtime, app loop, rendering, tools, content pipeline.
  - `crates/game`: game entrypoint, logging init, scene gameplay rules.
  - `assets/base`: base content XML source.
  - `mods`: mod folders (XML source per mod).
  - `cache`: runtime-generated content cache.
  - `docs`: specs, fixtures, operational docs.
- Source-controlled:
  - Rust source, docs, fixtures, `assets/base`, `mods` scaffolding.
- Runtime-generated:
  - `cache/content_packs/*.pack`
  - `cache/content_packs/*.manifest.json`
  - build artifacts under `target/`.

## 3) How to run
- Run command: `cargo run`.
- Expected startup behavior:
  - resolves app paths
  - builds/loads content database
  - opens window and runs fixed-step simulation + render loop
  - exits via `Esc` or window close.
- Root resolution:
  - if `PROTOGE_ROOT` is set, use it after repo-marker validation.
  - else walk upward from executable directory and choose first repo-marker match.
  - repo marker is `Cargo.toml` plus either `crates/` or `assets/`.
- Controls:
  - Move: `W`, `A`, `S`, `D` and arrow keys.
  - Camera pan: `I`, `J`, `K`, `L`.
  - Scene switch: `Tab` (edge-triggered).
  - Overlay toggle: `F3` (edge-triggered).
  - Quit: `Esc` or window close.
- Optional env vars:
  - `PROTOGE_ENABLED_MODS`: comma-separated, ordered enabled mod IDs.
  - `PROTOGE_SLOW_FRAME_MS`: artificial per-frame delay for clamp testing.

## 4) Engine vs game seam rules
- Engine owns:
  - window/event loop (`winit`)
  - rendering backend (`pixels`)
  - raw input capture and action mapping
  - fixed-step timing and lifecycle orchestration
  - path resolution and content planning/compile/load pipeline
  - overlay drawing and runtime metrics plumbing.
- Game owns:
  - `tracing_subscriber` initialization
  - scene gameplay behavior and state
  - optional debug title content via `Scene::debug_title`.
- Hard rules:
  - runtime simulation never parses XML.
  - deterministic ordering is required for discovery, merge, and ID assignment.
  - simulation updates run with fixed timestep principles.

## 5) Runtime loop
- Loop API surface:
  - `run_app`, `run_app_with_metrics`, `LoopConfig`, `AppError`.
- Fixed timestep model:
  - accumulator + `fixed_dt` from `target_tps`.
  - render cadence is separate from simulation ticks.
- Clamp and anti-spiral controls:
  - frame delta clamp via `max_frame_delta`.
  - per-frame tick cap via `max_ticks_per_frame`.
  - excess backlog is dropped and logged (`sim_clamp_triggered`).
- Quit behavior:
  - `WindowEvent::CloseRequested` and `Esc` trigger shutdown.
- Metrics surface:
  - `MetricsHandle` and `LoopMetricsSnapshot { fps, tps, frame_time_ms }`.
  - periodic structured logs emit loop metrics.

## 6) Scenes and scene switching
- Scene contract (`Scene` trait):
  - `load(&mut self, &mut SceneWorld)`
  - `update(&mut self, fixed_dt_seconds, &InputSnapshot, &mut SceneWorld) -> SceneCommand`
  - `render(&mut self, &SceneWorld)`
  - `unload(&mut self, &mut SceneWorld)`
  - optional `debug_title(&self, &SceneWorld) -> Option<String>`.
- Scene switching:
  - `SceneCommand::{None, SwitchTo(SceneKey)}`.
  - engine `SceneMachine` owns switching lifecycle.
  - switch order is `unload -> world.clear -> load`.
- Tab edge-trigger enforcement:
  - in `InputCollector` (`crates/engine/src/app/loop_runner.rs`).
  - `switch_scene_pressed` is one-frame true on press edge only.

## 7) Entities and world model
- `EntityId` scope:
  - monotonic allocator for session lifetime, no ID reuse after scene switches.
- Transform/world conventions:
  - `Transform { position: Vec2, rotation_radians: Option<f32> }`.
  - Y-up world convention used by world-to-screen transform.
- Queue semantics:
  - spawns and despawns are buffered then applied via `SceneWorld::apply_pending()`.
- `SceneWorld::clear()`:
  - clears entities, pending queues, and camera.
  - preserves compiled `DefDatabase` resource.
- Camera resource:
  - owned in `SceneWorld` as `Camera2D`.
  - accessed via `camera()` and `camera_mut()`.

## 8) Input system
- Actions (`InputAction`):
  - `MoveUp`, `MoveDown`, `MoveLeft`, `MoveRight`
  - `CameraUp`, `CameraDown`, `CameraLeft`, `CameraRight`
  - `ToggleOverlay`
  - `Quit`.
- Scene-side input surface:
  - `InputSnapshot::is_down(action)`
  - `InputSnapshot::switch_scene_pressed()`
  - `InputSnapshot::quit_requested()`.
- Mappings from physical keys:
  - move: `WASD` and arrows.
  - camera: `IJKL`.
  - overlay: `F3`.
  - quit: `Esc`.
  - scene-switch edge: `Tab`.
- Edge vs down-state:
  - down-state tracked for actions.
  - one-frame edges are tracked for `Tab` and `F3`.

## 9) Rendering
- Backend and ownership:
  - `Renderer` uses `pixels` + `winit`.
  - renderer consumes `SceneWorld` entities and camera; no scene render API coupling added.
- World-to-screen transform:
  - `screen_x = (world_x - camera_x) * pixels_per_world + width/2`
  - `screen_y = height/2 - (world_y - camera_y) * pixels_per_world`.
- Scale:
  - current constant is `PIXELS_PER_WORLD = 32.0`.
- Resize handling:
  - handles `Resized` and `ScaleFactorChanged`.
  - resizes both surface and buffer.
- Placeholder rendering:
  - `RenderableKind::Placeholder` draws simple square marker.

## 10) Tools overlay
- Data sources:
  - `MetricsHandle::snapshot()`
  - `SceneWorld::entity_count()`
  - content status placeholder string (`"loaded"` currently passed by loop).
- Rendering approach:
  - small fixed bitmap glyph set in `tools/overlay.rs`.
  - strict clipping-safe per-pixel writes with bounds checks.
- Toggle behavior:
  - `F3` edge-trigger toggles visibility.

## 11) Content pipeline overview
- Contract document:
  - `docs/content_pipeline_contract_v1.md` is the v1 boundary spec.
- Ticket 7 planning rails:
  - `build_compile_plan(app_paths, request)` produces deterministic per-mod decisions.
  - hashes include ordered enabled mods and normalized-path XML input hashes.
  - manifest path: `cache/content_packs/<mod_id>.manifest.json`.
  - pack path: `cache/content_packs/<mod_id>.pack`.
- Ticket 8 compiler:
  - strict XML parsing for `EntityDef`.
  - compile errors include code, message, mod ID, file path, best-effort line/column.
  - runtime output is `DefDatabase` with numeric IDs.
- Ticket 9 pack cache:
  - custom little-endian binary pack with payload SHA-256 integrity check.
  - atomic write order is pack first, then manifest (each temp-file + rename).
  - manifest is planning authority; loader also cross-checks redundant pack header metadata.
  - per-mod cache failure falls back to recompile.
- Ticket 10 overrides:
  - field-level merge in load order.
  - scalar fields last-writer-wins.
  - `tags` list replacement when present.
  - partial override with missing prior target fails fast (`MissingOverrideTarget`).
  - `pack_format_version` is the single binary format version signal; current value is `2`.

## 12) Practical workflows
- Add and enable a mod:
  - create folder `mods/<mod_id>/` with XML files.
  - set `PROTOGE_ENABLED_MODS` to ordered mod IDs, e.g. `moda,modb`.
  - run `cargo run` and verify content plan logs.
- Test cache invalidation:
  - edit one XML file in a mod and rerun; only that mod should rebuild.
  - corrupt a `.pack` file and rerun; that mod should rebuild via fallback.
- Add a new `InputAction` safely:
  - add enum variant and index mapping in `app/input.rs`.
  - map physical keys in `InputCollector`.
  - expose/use it through `InputSnapshot`.
  - add or update unit tests for mapping and behavior.
- Add a new Def field safely:
  - update XML parser and strict validation in compiler.
  - define merge semantics (scalar replace, list replace, etc.).
  - extend runtime `EntityArchetype`/database model.
  - extend pack encode/decode payload.
  - bump `pack_format_version` when binary schema changes.
  - ensure planner/manifest compatibility logic remains exact-match.
  - add compiler/pack/pipeline tests and fixture expectation updates.

## 13) Known tech debt and do-not-touch-yet
- Current tech debt:
  - `loop_runner.rs` leaks the `Window` reference via `Box::leak(...)` to satisfy current `winit` + `pixels` lifetime usage in the event-loop closure.
- Why it exists:
  - current architecture prioritizes stable prototype loop behavior over immediate lifetime refactor complexity.
- Future refactor goal:
  - eliminate leaked window by moving to a proper owned app-state lifecycle that satisfies lifetimes without leaking.
- Do-not-touch-yet guidance:
  - avoid broad event-loop architecture rewrites before next playable milestone.
  - keep simulation single-threaded and deterministic-first.
  - avoid large framework refactors that jeopardize vertical-slice momentum.

## 14) Quick checklist for changes
- Run `cargo fmt --all`.
- Run `cargo test --workspace`.
- Verify determinism-sensitive behavior:
  - fixed-step changes, ordering, merge rules, and ID stability.
- If pack binary schema changes:
  - bump `pack_format_version`
  - validate cache invalidation path.
- Ensure runtime paths still never parse XML.
- Update `README.md`, fixtures, and `CODEXNOTES.md` when contracts/interfaces change.
