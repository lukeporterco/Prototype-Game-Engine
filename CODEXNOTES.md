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
- Detailed Ticket 31-32.3 notes were moved to CODEXNOTES_ARCHIVE.md on 2026-02-20.

## Module Boundaries and Ownership
### A. Module map
#### Core
#### App/Loop
#### SceneMachine and Scene
#### World (SceneWorld and runtime state)
#### Rendering
#### Assets and Content Pipeline
#### Input
#### Tools (Overlay, Console)
#### Placeholders (Physics, Audio, Scripting seam)
### B. Ownership rules
Core owns shared primitive types, IDs, math/value helpers, and cross-cutting error/reporting contracts; Core must not own gameplay rules, runtime scene state, rendering orchestration, or tool command routing; Core may call standard library helpers and internal pure utility helpers; Core must not call App/Loop, SceneMachine/Scene, World, Rendering, Assets/Content, Input, or Tools runtime flows; data it is allowed to mutate is limited to its own local values and internal utility state with no mutation of scene/world runtime data.

App/Loop owns process lifecycle, window/event integration, fixed-timestep timing, frame pacing, and command routing between active scene and engine shell; App/Loop must not own gameplay rules, entity definitions, content compilation decisions, or renderable gameplay state; App/Loop may call SceneMachine and Scene entry points, Input sampling, Rendering entry points, and Tools surface hooks; App/Loop must not call gameplay mutation paths directly except through scene-facing APIs/debug hooks; data it is allowed to mutate is loop-local timing state, routing queues, and lifecycle flags, not gameplay world internals.

SceneMachine and Scene owns scene lifecycle transitions, active-scene selection, per-scene world persistence semantics, gameplay rule execution, and scene-scoped command handling; SceneMachine and Scene must not own low-level loop timing, renderer internals, content file parsing, or platform event pumping; SceneMachine and Scene may call World mutation/apply APIs, read input snapshots, and issue render requests through rendering seam contracts; SceneMachine and Scene must not call platform loop internals or bypass world deferred-apply semantics; data it is allowed to mutate is scene-owned gameplay state and scene-world command intent through defined apply points.

World (SceneWorld and runtime state) owns runtime entity/state storage, deferred mutation queues, camera/world data containers, and deterministic apply points for pending mutations; World (SceneWorld and runtime state) must not own high-level scene policy decisions, loop pacing policy, content authoring sources, or console shell control flow; World (SceneWorld and runtime state) may call internal data-structure helpers and expose read/write APIs consumed by Scene and read-only APIs consumed by Rendering/Tools; World (SceneWorld and runtime state) must not call App/Loop control paths or content compilation pipeline; data it is allowed to mutate is authoritative runtime state and queued/deferred mutation buffers only.

Rendering owns frame composition, camera/world projection, and draw-order execution from world snapshots and overlay/tool inputs; Rendering must not own gameplay decision logic, entity simulation state mutation, scene transition policy, or content compilation; Rendering may call read-only world accessors, camera transforms, and asset lookup/render backends; Rendering must not call scene gameplay mutation APIs or loop command routing mutation paths; data it is allowed to mutate is renderer-local transient buffers/caches and GPU/frame resources, not gameplay world state.

Assets and Content Pipeline owns mod/content discovery, XML authoring compile process, cache/manifest validation, and runtime DefDatabase loading from compiled packs; Assets and Content Pipeline must not own live gameplay runtime mutation, per-frame loop logic, or renderer gameplay decisions; Assets and Content Pipeline may call filesystem I/O, hashing, compiler/planner stages, and pack/database loaders; Assets and Content Pipeline must not call simulation update loops or parse XML at runtime during simulation; data it is allowed to mutate is content cache artifacts, compile outputs, and content database state during load/refresh boundaries.

Input owns raw input sampling normalization into deterministic frame/tick snapshots and action-mapping state used by loop/scene update calls; Input must not own gameplay rule execution, scene switching policy, renderer behavior, or content compilation; Input may call platform input event adapters and provide snapshots to App/Loop and Scene; Input must not call gameplay mutation functions directly or invoke scene command handlers on its own; data it is allowed to mutate is input collector/internal edge-trigger state and snapshot buffers.

Tools (Overlay, Console) owns debug UI text/metrics presentation and console command parsing/dispatch wiring for developer workflows; Tools (Overlay, Console) must not own core gameplay systems, loop timing ownership, renderer architecture policy, or content pipeline policy; Tools (Overlay, Console) may call read-only inspection paths and scene debug hook interfaces for explicit debug mutations; Tools (Overlay, Console) must not call loop runner gameplay mutation paths directly or bypass scene debug routing; data it is allowed to mutate is tool-local UI/command history state and explicit debug command payloads routed through approved seams.

Placeholders (Physics, Audio, Scripting seam) owns reserved extension seams and interface placeholders that define where future systems integrate without changing current ownership boundaries; Placeholders (Physics, Audio, Scripting seam) must not own current vertical-slice runtime logic before those modules are implemented, and must not introduce hidden dependency reversals; Placeholders (Physics, Audio, Scripting seam) may call only seam-definition helpers and compile-time contracts needed to keep boundaries explicit; Placeholders (Physics, Audio, Scripting seam) must not call active gameplay mutation paths until implemented under dedicated tickets; data it is allowed to mutate is placeholder-local configuration/contract descriptors only.
### C. Seam invariants
- Loop owns timing and command routing.
- Scene owns gameplay rules.
- World owns runtime state and deferred mutation apply points.
- Renderer is read-only over world state and camera; no gameplay mutation.
- Content is compiled ahead of runtime; runtime must not parse XML.
- Save/load is validation-first; on failure, no partial world mutation occurs.
- Scene switching semantics remain as implemented (persistent worlds per scene; explicit hard reset path).
- Console commands that mutate gameplay route through a scene debug hook, not through loop runner directly.

## Ticket 33 Systems Seam (2026-02-21)
- `GameplayScene` now owns a `GameplaySystemsHost` lane in `crates/game/src/main.rs` and runs it exactly once per tick before a single post-host mutation apply point.
- Deterministic system order is explicit and fixed: `InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup` (ASCII-only).
- Systems receive `GameplaySystemContext` with `WorldView` (read-only wrapper over `SceneWorld`), tick input, and scene-local event/intent buffers.
- Systems do not mutate `SceneWorld` directly; world/entity mutations are applied only in `apply_gameplay_tick_at_safe_point`.
- Debug seam change: `DebugInfoSnapshot.system_order` is now owned `String` and overlay shows `sys: <order>`.

