# CODEXNOTES.md
## Purpose
Living, structured notes for Codex and humans to keep consistent context across threads.
Keep this concise and actionable. Prefer bullet points. Avoid long code dumps.
- Historical ticket logs were moved to `CODEXNOTES_ARCHIVE.md` on 2026-02-20.
- Use `CODEXNOTES.md` for active context only; use `CODEXNOTES_ARCHIVE.md` for historical detail.
---
## Decisions (Locked)
- Language/runtime: Rust
- Content authoring: XML
- Content runtime: compiled binary ContentPack
- Compile behavior: compile-on-first-launch, cache per mod, rebuild on change or invalid cache
- Simulation: fixed timestep (TPS), deterministic mindset
- Vertical slice priorities: boot -> scene -> entity move -> render -> debug overlay -> clean quit
---
## Current Milestone
- Milestone: Vertical Slice v0
- Definition: window + loop + scene + controllable entity + render + overlay + clean quit
### Next Tickets (Queue)
- Tickets 0-30 completed.
- Next queue: pending human prioritization.
---
## Module Map (Ownership)
- Core: common types, IDs, error/reporting primitives
- App/Loop: window init, main loop, fixed timestep, lifecycle
- Scene: scene lifecycle, entity storage, spawn/despawn, world state container
- Rendering: camera, renderables, world-to-screen
- Input: action mapping, per-frame input sampling
- Assets: asset handles/paths and sprite lookup seam
- Tools: debug overlay, counters, diagnostics
- Content: mod scan, XML compile, pack I/O, DefDatabase
---
## Data Contracts (Plain English)
### Scene and Entity (runtime)
- Entity: stable ID + Transform + Renderable descriptor + runtime order state
- Transform: position (2D), rotation_radians
- Scene API: load / update(fixed_dt, input, world) / render(world) / unload(world)
- Scene owns entity list and spawn/despawn rules
### Content and Mods
- Mod: folder with XML files (and optionally art assets)
- Load order: base content first, then enabled mods in configured order
- Override rule MVP: scalar fields last-writer-wins; list fields full replacement
### DefDatabase (runtime)
- Stores compiled defs/archetypes only (no runtime state)
- Uses numeric IDs internally; no runtime XML parsing
- SceneWorld holds DefDatabase resource across scene clears
### ContentPack v1 (cached binary)
- Per-mod cache file + per-mod manifest
- Includes deterministic ordering and cache-key fields for validation
- Cache/schema compatibility keyed by pack format version
### Save/Load (runtime)
- Save schema version: v3
- Runtime entity references persist via stable save IDs (not transient entity indices)
- Validation-first restore: parse/validate before mutating world/scene state
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
- Content errors fail fast at load (don't limp into runtime with partial state)
- Keep logs structured and minimal in hot paths
---
## Pitfalls and Gotchas (Update as found)
- Save restore paths that rebuild role-based runtime state require DefDatabase to be present.
---
## Known Issues / TODO
- Next ticket queue not defined beyond Ticket 30; awaiting prioritization.
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
