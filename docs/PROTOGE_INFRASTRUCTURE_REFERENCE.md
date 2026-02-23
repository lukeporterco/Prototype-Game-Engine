# PROTOGE Infrastructure Reference
Last updated: 2026-02-23. Covers: Tickets 0-52.

## 1) Project snapshot
- Proto GE is a prototype-first Rust engine + colony-sim vertical slice.
- Current implementation baseline reflects work through Ticket 52.
- Vertical slice status:
  - fixed-step loop and scene lifecycle
  - entity control/selection/orders/jobs
  - sprites + tilemap + affordance rendering
  - save/load with stable save IDs (v3)
  - camera zoom + FPS cap controls

## 2) Repository map
- `crates/engine`
  - app loop, input snapshot/collector, scene runtime, renderer, overlay tools
  - content pipeline: discovery, compile, binary pack cache, runtime database
- `crates/game`
  - gameplay scene logic, save/load DTOs and restore flow
- `assets/base`
  - base XML defs and sprite assets
- `mods`
  - optional XML content mods
- `cache`
  - runtime-generated content packs/manifests/saves
- `docs`
  - contracts and infrastructure references

## 3) Runtime and controls
- Run command: `cargo run` (workspace default member is `crates/game`).
- Quit:
  - `Esc`
  - window close
- Core controls:
  - move actor: `W/A/S/D` or arrows
  - camera pan: `I/J/K/L`
  - zoom (discrete): mouse wheel, `=`, `-`, numpad `+/-`
  - switch scene: `Tab` (edge-triggered)
  - overlay toggle: `F3` (edge-triggered)
  - save/load: `F5` / `F9` (edge-triggered)

## 4) App loop and pacing contracts
- Public loop API:
  - `run_app`, `run_app_with_metrics`, `LoopConfig`, `AppError`
- `LoopConfig` key pacing fields:
  - `target_tps`, `max_frame_delta`, `max_ticks_per_frame`
  - `fps_cap: Option<u32>` (authoritative FPS cap knob)
- Pacing rules:
  - fixed-step simulation with accumulator
  - render cadence is decoupled from sim ticks
  - exactly one FPS-cap sleep point in redraw path (`compute_cap_sleep` + `thread::sleep`)
  - debug frame delay (`PROTOGE_SLOW_FRAME_MS`) is explicit perturbation, not hidden cap logic
- Startup logging:
  - emits `loop_config` with effective cap; uncapped displays as `U+221E` (infinity)

## 5) Window/renderer ownership seam
- Loop runner owns an `Arc<Window>` and shares clones.
- `Renderer` is lifetime-free and owns:
  - `window: Arc<Window>`
  - `pixels: Pixels<'static>`
- No `Box::leak` usage remains for window lifetime handling.
- Resize behavior:
  - renderer rebuilds pixels surface from owned window handle on resize.

## 6) Scene and world model
- `Scene` trait:
  - `load`, `update`, `render`, `unload`
  - optional debug methods for title/snapshots
- `SceneMachine`:
  - two persistent scene slots with per-scene `SceneWorld`
  - normal switch (`SwitchTo`) is non-destructive pointer switch
  - explicit hard reset (`HardResetTo`) performs unload/clear/reload
- Only active scene world ticks; inactive worlds remain paused.

## 7) Entity/runtime data model
- `SceneWorld` owns:
  - entity storage + spawn/despawn queues
  - camera (`Camera2D`)
  - optional tilemap
  - transient visual state and debug markers
  - optional `DefDatabase` handle
- `RenderableKind`:
  - `Placeholder`
  - `Sprite(String)`
- Picking semantics:
  - deterministic topmost winner by last applied spawn order
  - sprite renderables and placeholder renderables are both pickable

## 8) Rendering pipeline
- Render order:
  - clear
  - tilemap ground layer
  - world grid debug
  - entities
  - affordances (selection/hover/order marker)
  - overlay
- Sprite asset seam:
  - entity/tile sprite resolution path: `asset_root/base/sprites/<key>.png`
  - sprite keys are validated (`a-z0-9_/-`, non-empty, rejects `..`, leading `/`, `\`)
  - invalid/missing/failed decode falls back to placeholder/solid fallback
- Tilemap v0:
  - optional `SceneWorld`-owned tilemap with `u16` tile IDs
  - origin convention:
    - `origin` is world position of tile `(0,0)` bottom-left corner
    - tile center = `origin + (x + 0.5, y + 0.5)`
  - `SceneWorld::clear()` preserves tilemap; `clear_tilemap()` removes it

## 9) Interaction affordances
- Scene visual state uses weak `EntityId` references:
  - selected actor visual
  - hovered interactable visual
- Renderer skips unresolved weak IDs safely.
- Selected highlight draws only for live actor entities.
- Order markers:
  - scene-owned debug markers with TTL (`0.75s` for order marker)
  - marker ticking is single-pass decrement + retain.

## 10) Camera and transforms
- `Camera2D` includes:
  - position
  - zoom scalar
- Zoom contract:
  - discrete step zoom with clamps (`min/default/max/step`)
  - zoom keys are edge-triggered only
  - input collector accumulates zoom step deltas only
  - camera zoom mutation occurs in gameplay update before screen-to-world conversions
- Projection seam:
  - `world_to_screen_px` and `screen_to_world_px` use same effective pixels-per-world and remain strict inverse within tolerance.

## 11) Save/load (v3 stable references)
- Save version:
  - `SAVE_VERSION = 3`
- Save references are stable IDs, not entity indices:
  - each saved entity carries `save_id: u64`
  - refs use `*_save_id` fields (selected/player/interaction/job target)
- `SaveGame.next_save_id: u64` is persisted.
- Gameplay runtime maintains:
  - `entity_save_ids: HashMap<EntityId, u64>`
  - `next_save_id: u64`
- Sync guarantees:
  - existing IDs are never reassigned/resequenced
  - IDs assigned only to saved entities missing one
  - deterministic assignment for missing IDs
- Restore safety:
  - validation-first checks (including save-id integrity and `next_save_id` constraints)
  - world mutation occurs only after validation passes.

## 12) Content pipeline
- Runtime never parses XML.
- Startup uses build-or-load content workflow:
  - deterministic mod discovery and compile planning
  - per-mod binary content pack cache + manifest
  - cache corruption/mismatch triggers selective rebuild
- Renderable authoring (Ticket 22):
  - preferred attribute form:
    - `<renderable kind="Placeholder" />`
    - `<renderable kind="Sprite" spriteKey="player" />`
  - legacy text form still accepted
  - if `kind` attribute exists, it wins
  - `kind` + non-whitespace text is compile error
  - unknown renderable attributes are compile errors.

## 13) Overlay/debug contracts
- Overlay FPS line format:
  - `[{current} / {cap}] dbg+{slow_ms}ms`
  - uncapped cap value is `U+221E` (infinity)
- Overlay includes:
  - FPS/TPS/frame time/entity/content
  - selection/target/resource info
  - inspect block fields for selected runtime state.

## 14) Environment variables
- `PROTOGE_ROOT`
  - explicit root override
- `PROTOGE_ENABLED_MODS`
  - ordered comma-separated mod list
- `PROTOGE_SLOW_FRAME_MS`
  - explicit per-frame debug delay

## 15) Known boundaries
- Keep simulation deterministic-first and single-threaded.
- Avoid broad architecture refactors outside ticket scope.
- Do not introduce runtime XML parsing.

## 16) Practical verification checklist
- Formatting:
  - `cargo fmt --all -- --check`
- Tests:
  - `cargo test -p engine`
  - `cargo test -p game`
- Spot checks:
  - uncapped overlay/log shows `U+221E` (infinity)
  - scene switch preserves per-scene runtime state
  - save/load restores stable references across spawn-order differences.
