# PROTOGE Infrastructure Reference

## 1) Project snapshot
- Proto GE is a prototype-first Rust engine plus colony-sim vertical slice scaffold.
- Current status is Vertical Slice v0 with loop, scene runtime, entity interaction loop, rendering, overlay, and save/load v0.
- Ticket outcomes (0-16):
  - Ticket 0: workspace bootstrap, canonical dirs, deterministic root discovery.
  - Ticket 1: fixed-timestep loop, lifecycle, structured metrics.
  - Ticket 2: scene lifecycle seam and engine-owned world/entity model.
  - Ticket 3: action-based input seam and controllable movement.
  - Ticket 4: `pixels` renderer + camera/world-to-screen seam.
  - Ticket 5: clipping-safe debug overlay and `F3` toggle.
  - Ticket 6: content pipeline contract spec.
  - Ticket 7: deterministic mod discovery + compile planning.
  - Ticket 8: strict XML compiler to runtime `DefDatabase`.
  - Ticket 9: binary content-pack cache with fallback rebuild.
  - Ticket 10: field-level override merge semantics.
  - Ticket 11: selection seam and deterministic picking order.
  - Ticket 12: actor move targets and right-click order loop.
  - Ticket 13: interactable + timed job completion loop.
  - Ticket 14: inspect snapshot block in overlay.
  - Ticket 14.5: persistent per-scene runtime worlds + explicit hard reset path.
  - Ticket 15: world-space renderer grid debug pass.
  - Ticket 16: save/load v0 (JSON, per-scene files, validation-first restore).

## 2) Repository map
- Primary folders:
  - `crates/engine`: app loop, input, scene runtime, rendering, overlay, content pipeline.
  - `crates/game`: gameplay rules, scene behavior, save/load v0.
  - `assets/base`: base XML authoring source.
  - `mods`: mod XML source.
  - `cache`: runtime-generated data.
  - `docs`: specs/fixtures/reference docs.
- Runtime-generated outputs:
  - `cache/content_packs/*.pack`
  - `cache/content_packs/*.manifest.json`
  - `cache/saves/scene_a.save.json`
  - `cache/saves/scene_b.save.json`
  - build artifacts under `target/`.

## 3) Run and controls
- Run command: `cargo run`.
- Startup sequence:
  - resolve app paths
  - build/load compiled content database
  - initialize scenes + renderer
  - run fixed-step sim + decoupled render
  - exit via `Esc` or window close.
- Controls:
  - Move: `W/A/S/D` and arrow keys.
  - Camera pan: `I/J/K/L`.
  - Scene switch: `Tab` (edge-triggered).
  - Overlay toggle: `F3` (edge-triggered).
  - Save: `F5` (edge-triggered).
  - Load: `F9` (edge-triggered).
  - Quit: `Esc` or window close.
- Optional env vars:
  - `PROTOGE_ENABLED_MODS` (ordered, comma-separated).
  - `PROTOGE_SLOW_FRAME_MS` (artificial frame delay).
  - `PROTOGE_ROOT` (explicit root override).

## 4) Engine vs game seam rules
- Engine owns:
  - window/event loop (`winit`) and renderer (`pixels`)
  - raw input collection + action/edge snapshots
  - fixed-step timing and clamp behavior
  - scene runtime orchestration (`SceneMachine`)
  - content planning/compile/load pipeline
  - overlay rendering.
- Game owns:
  - gameplay logic/state transitions in `Scene` implementation
  - selection/orders/jobs/interactable loop
  - save/load DTOs and disk IO policy
  - debug title/snapshot content.
- Hard rules:
  - runtime simulation never parses XML.
  - deterministic fixed-step simulation mindset.
  - no broad architecture refactors outside ticket scope.

## 5) Runtime loop
- API surface:
  - `run_app`, `run_app_with_metrics`, `LoopConfig`, `AppError`.
- Fixed timestep:
  - accumulator + `fixed_dt` from `target_tps`.
  - render cadence separate from simulation ticks.
- Anti-spiral controls:
  - frame delta clamp (`max_frame_delta`)
  - per-frame tick cap (`max_ticks_per_frame`)
  - dropped backlog warning log (`sim_clamp_triggered`).
- Metrics:
  - `MetricsHandle` and `LoopMetricsSnapshot { fps, tps, frame_time_ms }`.
- Shutdown:
  - scene unload on exit via `SceneMachine::shutdown_all()`.

## 6) Scenes and switching semantics
- `Scene` trait contract:
  - `load`, `update`, `render`, `unload`
  - optional debug methods (`debug_title`, selection/target/resource/debug snapshot seams).
- `SceneMachine` runtime model:
  - per-scene runtime slot: `scene + world + is_loaded`.
  - each scene has a persistent `SceneWorld`.
- Switching behavior:
  - `SceneCommand::SwitchTo(SceneKey)`:
    - non-destructive pointer switch
    - load target once if never loaded
    - no unload/clear/load on normal switch.
  - `SceneCommand::HardResetTo(SceneKey)`:
    - explicit `unload -> clear -> load -> activate`.
- Pause semantics:
  - only active scene world ticks; inactive worlds do not advance movement/jobs/timers.

## 7) World/entity runtime model
- `SceneWorld` owns:
  - entities, pending spawn/despawn queues, camera, optional `DefDatabase`.
- Entity runtime fields include:
  - `id`, `transform`, `renderable`
  - selection/order/job fields: `selectable`, `actor`, `move_target_world`
  - interactable/job fields: `interactable`, `interaction_target`, `job_state`.
- Queue semantics:
  - `spawn*` and `despawn` enqueue; `apply_pending` commits.
- Picking semantics:
  - selectable/interactable screen hit tests use placeholder bounds.
  - overlap winner is deterministic: last applied spawn order wins.

## 8) Input model
- Down-state actions:
  - movement (`Move*`), camera pan (`Camera*`), overlay toggle action state, quit.
- Edge-trigger fields in `InputSnapshot`:
  - `switch_scene_pressed`
  - `left_click_pressed`
  - `right_click_pressed`
  - `save_pressed`
  - `load_pressed`.
- Mouse/window seam in `InputSnapshot`:
  - `cursor_position_px`
  - `window_size`.

## 9) Rendering
- Backend:
  - `Renderer` (`pixels`) consumes active `SceneWorld`.
- Projection seam:
  - `world_to_screen_px(camera, window_size, world_pos)`
  - `screen_to_world_px(camera, window_size, screen_px)` strict inverse.
- Placeholder rendering:
  - `RenderableKind::Placeholder` square markers.
- Grid debug pass (Ticket 15):
  - draw order: clear -> grid -> placeholders -> overlay.
  - world-space minor lines + major lines every 5 cells.
  - viewport-scoped line iteration only.
  - deterministic major classification for negative indices via Euclidean modulo.
  - clipping-safe per-pixel writes.

## 10) Overlay/debug
- Overlay shows:
  - FPS/TPS/frame/entity/content
  - selection line (`Sel`)
  - selected target (`Target`)
  - resource count (`items`)
  - inspect block from `DebugInfoSnapshot` (`Inspect`, `sel`, `pos`, `ord`, `job`, counts).
- Overlay text renderer:
  - small bitmap glyph set
  - clipping-safe blitter with bounds checks.

## 11) Gameplay loop (current micro-loop)
- Selection:
  - left click selects actor; empty click clears.
- Orders:
  - right click with selected actor:
    - interactable-first behavior
    - else regular move target.
- Jobs:
  - actor enters interaction radius, starts timed work.
  - completion increments resource counter and decrements/despawns interactable uses.
- Scene persistence:
  - switching away/back preserves active world state exactly unless hard reset path is used.

## 12) Save/load v0
- Scope:
  - active scene save/load only, per-scene file path in `cache/saves`.
- Format:
  - JSON with integer `save_version`.
- Data policy:
  - runtime-state focused payload:
    - transforms/camera
    - selection/player refs
    - orders/job progress
    - interactable runtime fields
    - resource counters
  - references stored as saved entity indices (array positions), not raw runtime IDs.
- Load safety:
  - validate parse/version/scene key/index references before mutation.
  - if validation fails, current world state remains unchanged.
- Reconstruction:
  - static renderable/config is rebuilt from compiled defs/archetypes where applicable.
  - runtime restore does not parse XML.

## 13) Content pipeline overview
- Authoring/runtime split:
  - XML authoring -> compiled binary packs -> runtime `DefDatabase`.
- Determinism:
  - ordered source discovery/hash/merge rules.
- Overrides:
  - field-level merge (`tags` list replace, scalars last-writer-wins).
- Cache:
  - per-mod binary pack + manifest under `cache/content_packs`.
  - manifest exact-match checks and pack integrity checks.
  - bad cache falls back to recompile for affected mods.

## 14) Known tech debt / do-not-touch-yet
- `Window` lifetime is currently satisfied via `Box::leak(...)` in loop runner.
- Keep simulation single-threaded and deterministic-first.
- Avoid broad event-loop or ECS-style refactors before next playable milestone.

## 15) Practical checklist for changes
- Run:
  - `cargo fmt --all`
  - `cargo test --workspace`
- Validate seam contracts:
  - fixed-step determinism and ordering-sensitive behavior.
  - no runtime XML parsing introduced.
- If binary content pack schema changes:
  - bump `pack_format_version`
  - verify cache invalidation/fallback behavior.
- Keep docs in sync:
  - `README.md`
  - `CODEXNOTES.md`
  - relevant contract docs/fixtures.
