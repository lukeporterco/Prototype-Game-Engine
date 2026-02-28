# CODEXNOTES.md
Last updated: 2026-02-28. Covers: Tickets 0-54.
## Purpose
Living, structured notes for Codex and humans to keep consistent context across threads.
Keep this concise and actionable. Prefer bullet points. Avoid long code dumps.
- Historical ticket logs were moved to `CODEXNOTES_ARCHIVE.md` on 2026-02-20.
- Use `CODEXNOTES.md` for active context only; use `CODEXNOTES_ARCHIVE.md` for historical detail.
- Canonical thruport startup/session runbook lives at `.codex_artifacts/SOME_COMMANDS.md`.
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
- Detailed Ticket 31-32.3 notes were moved to CODEXNOTES_ARCHIVE.md on 2026-02-20.

- Deprecated in-place detailed notes (Module Boundaries + Tickets 33-47) were moved to `CODEXNOTES_ARCHIVE.md` on 2026-02-23.

- Status model reminder: statuses use `StatusId(&'static str)` and shipping slow status id is `status.slow`.
- Deprecated in-place detailed notes (Module Boundaries + Tickets 48-54 + single-shot stress update) were moved to `CODEXNOTES_ARCHIVE.md` on 2026-02-28.

- Ticket 55 (2026-02-28): renderer now owns a micro-grid snap policy for world-space draw placement (`crates/engine/src/app/rendering/renderer.rs`), defaulting to `MICRO_GRID_RESOLUTION_PX = 1`; simulation transforms and picking logic remain unchanged.
- Ticket 56 (2026-02-28): added floor-layer runtime contract with `FloorId` (Rooftop/Main/Basement), `Entity.floor`, and `SceneWorld.active_floor`; topmost pick functions now take optional floor filters (`None` keeps legacy behavior) and `floor.set` routes as an engine queueable command to scene-owned behavior.
- Ticket 57 (2026-02-28): gameplay orderability contract now restricts job/order commands (`order.move`, `order.interact`, right-click job intents) to the authoritative `player_id`; non-player NPC actors remain selectable but are non-jobbable, and combat AI auto-registration now requires archetype combat fields instead of applying to every non-player actor.

## Module Boundaries and Ownership
### A. Module map
#### Core
- Shared IDs, value types, and cross-module contracts.
#### App/Loop
- Main loop, window/input pump, scene routing, and queueable command execution.
#### SceneMachine and Scene
- Scene lifecycle (`load/update/render/unload`) and debug-command seam.
#### World (SceneWorld and runtime state)
- Runtime entities, camera, tilemap, visuals, debug markers, and pick helpers.
#### Rendering
- Projection, world pass draw policy, sprite/tile draw, and overlay/console composition.
#### Assets and Content Pipeline
- XML discovery/compile, cache planning, and DefDatabase runtime load path.
#### Input
- Action snapshots and edge-trigger semantics for simulation-safe input use.
#### Tools (Overlay, Console)
- In-game console parsing/queueing and debug overlay text/perf presentation.
#### Placeholders (Physics, Audio, Scripting seam)
- Reserved seams only; no advanced subsystem ownership yet.
### B. Ownership rules
- Engine owns render policy, command parsing/routing, and scene machine orchestration.
- Game owns gameplay rules/state transitions and scene debug command behavior.
- Runtime simulation state mutates in gameplay safe points, not in rendering paths.
### C. Seam invariants
- Dependency direction remains engine -> game boundary-safe with scene trait seam.
- Rendering/picking policies may read runtime state but must not mutate simulation transforms.
- Console queueable outputs remain standardized as `ok:` or `error:` lines.
