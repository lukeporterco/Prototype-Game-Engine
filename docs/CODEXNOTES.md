# CODEXNOTES.md
Last updated: 2026-03-04. Covers: Tickets 0-70.1.
## Purpose
Living, structured notes for Codex and humans to keep consistent context across threads.
Keep this concise and actionable. Prefer bullet points. Avoid long code dumps.
- Ticket-by-ticket logs are written directly to `docs/CODEXNOTES_ARCHIVE.md` at ticket completion time.
- Use `docs/CODEXNOTES.md` for active living context only; use `docs/CODEXNOTES_ARCHIVE.md` for historical detail.
- `docs/CODEXNOTES.md` is not a temporary staging area for ticket logs.
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
## Module Boundaries and Ownership
### A. Module map
#### Core
- Owns shared primitives and IDs used across engine/game layers.
#### App/Loop
- Owns fixed-step loop orchestration, frame pacing, and scene-machine stepping.
#### SceneMachine and Scene
- Owns scene lifecycle (`load/update/render/unload`) and scene switching contracts.
#### World (SceneWorld and runtime state)
- Owns entity storage, transforms, runtime visual state, and spawn/despawn queues.
#### Rendering
- Owns camera/world-to-screen transforms and sprite/placeholder drawing policy.
#### Assets and Content Pipeline
- Owns XML-to-compiled-pack flow and runtime `DefDatabase` loading.
#### Input
- Owns input sampling and per-tick snapshots used by gameplay update.
#### Tools (Overlay, Console)
- Owns debug overlay, console, command palette, and debug command wiring.
#### Placeholders (Physics, Audio, Scripting seam)
- Reserved seams only; no full subsystem ownership yet.
### B. Ownership rules
- Engine layer must not depend on game rules.
- Runtime simulation must not parse XML.
- Scene/game logic can consume engine seams but should not mutate unrelated engine modules.
### C. Seam invariants
- Fixed-step simulation remains deterministic-first.
- Scene debug command routing stays explicit engine->scene boundary.
- Render entrypoints remain stable while visual behavior evolves behind the seam.
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
- Detailed Ticket 31-32.3 notes were moved to `docs/CODEXNOTES_ARCHIVE.md` on 2026-02-20.

- Deprecated in-place detailed notes (Module Boundaries + Tickets 33-47) were moved to `docs/CODEXNOTES_ARCHIVE.md` on 2026-02-23.
- Deprecated in-place detailed notes (Status model reminder + Tickets 55-65 + legacy Module Boundaries dump) were moved to `docs/CODEXNOTES_ARCHIVE.md` on 2026-03-01.
- Ticket 66 (2026-03-03): renderer sprite blit now uses deterministic integer source-over alpha blending (`alpha=0` no-op, `alpha=255` full replace, intermediate alpha blends RGB/A).
- Ticket 66 (2026-03-03): gameplay now updates `EntityActionVisual` for all actors per tick from actual movement delta; non-player actors hold facing on zero-delta and force `ActionState::Interact` when order-state indicates interaction.
- Ticket 67 (2026-03-03): interaction outcomes for tagged interactables moved to `CombatResolution` (`InteractionCompleted`-driven) and now emit explicit intents (`SetCarryVisual`, `ClearCarryVisual`, `DecrementInteractableUses`, `IncrementResourceCount`, `StartHitVisualTimer`); `CompleteInteraction` safe-point apply is mechanical-only (order idle + nav clear).
- Ticket 67 (2026-03-03): carry runtime store added (`GameplayScene.carry_visual_by_actor`) with save/load persistence via optional `SavedEntityRuntime.carry_visual_def` (`#[serde(default)]`, save schema remains v3 for backward compatibility).
- Ticket 67 (2026-03-03): deterministic scene controls expanded with settler multi-select drag box + stable right-click fan-out, 8-neighbor integer-cost A* with no corner-cutting, tilemap epoch-driven path staleness checks/repath, and job auto-pick priority/reservation-timeout policies.
- Ticket 67 (2026-03-03): input snapshot contract now includes left-button held/release edge (`left_mouse_held`, `left_click_released`) propagated through loop runner injection/native paths for deterministic drag selection handling.
- Ticket 68 (2026-03-03): renderer `UseTool` attachment now resolves anchors deterministically with fallback `tool -> hand -> skip` (carry path unchanged), preserving existing west-only mirror behavior.
- Ticket 68 (2026-03-03): procedural `Hit` and `UseTool` recoil plus `UseTool` flicker halo now use integer/fixed-point tick-phase LUTs seeded from stable `EntityId` (no float rate/amplitude constants for those effects), and halo writes reuse the same source-over blend math as sprite blit.
- Ticket 68 (2026-03-03): visual_sandbox-only bridge maps `workbench_demo` interactions to `ActionState::UseTool` and injects visual-only fallback held visual `proto.visual_carry_item` when not carrying; no carry runtime mutation and no command/probe/save schema changes.
- Ticket 69 (2026-03-03): command palette now supports file-backed macros loaded lazily from `cache/tools/command_palette_macros.v1.json` (JSON `version:1`) with caps `MAX_MACROS=24`, `MAX_COMMANDS_PER_MACRO=16`, and click-time queue cap `MAX_QUEUED_BYTES_PER_MACRO_CLICK=4096`; macro execution enqueues each command via the existing console pending-line path (no parser/routing bypass).
- Ticket 69 (2026-03-03): macro load failures are one-shot per run (`error: command palette macros load failed: <reason>`), missing macro file is silent/no-op, and over-cap files are rejected as invalid for the run (no macro buttons).
- Ticket 69 (2026-03-03): `DebugInfoSnapshot` gained typed `selected_role_text: Option<String>`; gameplay now populates it from `pawn_role_by_entity` for `PlayerPawn|Settler|Npc`, and overlay inspect renders a single compact `role: <value>` line only when present (no `dump.state v1`/`dump.ai v1` schema changes).
- Ticket 70 (2026-03-03): V2 backlog hygiene rule added: active items stay in `docs/V2_BACKLOG.md`; completed/obsolete items are moved (not copied) to `docs/V2_BACKLOG_ARCHIVE.md` with `Date` and `Closed-by` metadata.

