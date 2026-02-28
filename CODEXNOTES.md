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
- Ticket 57 (2026-02-28, occlusion assist v0): renderer affordances now support deterministic occlusion assist using renderer overlap order keys (`Entity::renderer_overlap_order_key`) and placeholder-sized screen bounds; `SceneVisualState` gained `targeted_interactable`, gameplay populates it from selected actor `Interact/Working` target on active floor, and renderer draws x-ray outline passes without changing simulation or picking.
- Ticket 58.1 (2026-02-28): added tiny visual-test sprite assets under `assets/base/sprites/visual_test/` and rewired `proto.player`, `proto.npc_chaser`, and `proto.npc_dummy` sprite keys to those assets; renderer sprite-load fallback remains unchanged, with new rate-limited once-per-key warning logs for missing/invalid/decode-failed sprite loads.
- Ticket 58.2 (2026-02-28): added gameplay scenario id `visual_sandbox` to `scenario.setup`, which deterministically clears live entities then spawns `proto.player`, `proto.resource_pile`, `proto.door_dummy`, and `proto.stockpile_small` at fixed on-screen coordinates with an intentional overlap at `(0,0)`; success payload schema is `scenario.setup visual_sandbox player:<id> prop:<id> wall:<id> floor:<id>`.
- Ticket 59 (2026-02-28): sprite renderables now support optional XML `pixelScale` (1..=16, default 1) carried through runtime and content pack (`CONTENT_PACK_FORMAT_VERSION` bumped to 3); renderer applies integer nearest-neighbor sprite enlargement via scaled draw dimensions while keeping pixel-snap placement and all gameplay/picking semantics unchanged.
- Ticket 60 (2026-02-28): save/load v3 remains backward-compatible but now carries optional floor/archetype identity fields (`SaveGame.active_floor`, `SavedEntityRuntime.floor`, `SavedEntityRuntime.archetype_def_name`); apply-save restores per-entity floors by temporarily setting `SceneWorld.active_floor` before each spawn and then reapplies saved scene active floor. Gameplay now tracks `entity_archetype_id_by_entity` and reuses `target_lookup_by_save_id` across ticks to avoid per-tick map reallocations, while restore/rebuild paths derive combat defaults from persisted archetype identity when available.
- Ticket 60 (2026-02-28, rendering/transport/content robustness): renderer world pass now consumes an explicit sorted visible draw list (`renderer_overlap_order_key` + `EntityId` tiebreak) instead of relying on entity storage order, and `draw_sprite_centered_scaled` now clips destination bounds once and uses precomputed inverse-scale row/column mapping. Content atomic writes now use a std-only backup-rename replacement flow (no delete gap), and thruport now enforces a per-client control-queue byte cap with oldest-control eviction while preserving control-before-telemetry queue ordering.
- Ticket 61.1 (2026-02-28): added dev-only Command Palette UI in engine tools (`crates/engine/src/app/tools/command_palette.rs`) that emits command strings into existing console pending-line parsing flow (no parser/routing semantics change). Palette supports immediate buttons plus single-use armed spawn placement (`spawn <def> x y`) with world-space click resolution, right-click cancel, and panel-hit exclusion from placement; spawn presets are sourced from active-world `DefDatabase` def names.

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
