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
