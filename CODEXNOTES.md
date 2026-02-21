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

## Ticket 34 Event Bus Seam (2026-02-21)
- `GameplayEventBus` in `crates/game/src/main.rs` is scene-local and supports same-tick chaining via `emit` + `iter_emitted_so_far`.
- End-of-tick lifecycle uses rollover (`finish_tick_rollover`) to snapshot `last_tick_counts` and clear current tick events.
- Event set now includes payload-bearing variants for upcoming phases: `InteractionStarted`, `InteractionCompleted`, `EntityDamaged`, `EntityDied`, `StatusApplied`, `StatusExpired`.
- `DebugInfoSnapshot` gained generic `extra_debug_lines: Option<Vec<String>>` so game-specific observability can be surfaced without engine event-specific fields.
- Overlay inspect block renders any `extra_debug_lines`; gameplay uses this for `ev:` and `evk:` per-last-tick event counts.

## Ticket 35 Intent Apply Seam (2026-02-21)
- `GameplayIntentQueue` is scene-local and is the only mutation request path used by systems; intents are applied exactly once per tick in `apply_system_outputs` via `apply_gameplay_intents_at_safe_point`.
- Intent set is explicit and ordered: `SpawnByArchetypeId`, `DespawnEntity`, `ApplyDamage`, `AddStatus`, `RemoveStatus`, `StartInteraction`, `CompleteInteraction`.
- Runtime game-owned component stores live in `GameplayScene`: `health_by_entity: HashMap<EntityId, u32>` and `status_ids_by_entity: HashMap<EntityId, Vec<StatusId>>`.
- `StatusId` is a dedicated newtype (`StatusId(u64)`) to reduce churn when statuses become data-driven.
- Death rule is explicit: `ApplyDamage` only mutates health store; despawn requires explicit `DespawnEntity`.
- `CompleteInteraction` is mechanical-only for now (actor order state update); no resource grant/depletion logic is baked into this intent.
- Invalid intent targets are non-panicking: skipped and counted in `invalid_target_count`.
- Debug observability uses generic overlay extra lines: `in:`, `ink:`, `in_bad:` plus `spawned_entity_ids` in apply stats for deterministic tests/debug only.
- Same-tick spawned-entity reference handles are intentionally not supported in intent payloads yet.

## Ticket 36 Interaction Seam (2026-02-21)
- Interaction flow is now system-driven with scene-local runtime state: `active_interactions_by_actor` and monotonic `InteractionId`.
- `InputIntent` starts interactions only (`InteractionStarted` event + `StartInteraction` intent), and never emits completion.
- `Interaction` system owns all completion paths (including duration `0.0` immediate) and emits `InteractionCompleted` + `CompleteInteraction` intent only on successful completion.
- `GameplayIntent::CancelInteraction { actor_id }` was added; all cancellation paths use it and `CompleteInteraction` is reserved for successful completion only.
- Safe-point apply handles `StartInteraction`, `CancelInteraction`, and `CompleteInteraction` mechanically via actor order-state transitions; invalid targets remain non-panicking.
- Legacy direct completion/depletion logic was removed from `apply_gameplay_tick_at_safe_point`; world mutation outcomes now flow through intents only.
- Overlay debug block now includes interaction runtime line: `ix: ...`, and intent-kind counters include cancellation as `ca` in `ink: ...`.

## Ticket 37 AI Lite Seam (2026-02-21)
- `GameplaySystemId::AI` now runs real logic in `crates/game/src/main.rs` via scene-owned `ai_agents_by_entity: HashMap<EntityId, AiAgent>`.
- AI state machine MVP states are `Idle`, `Wander`, `Chase`, and `UseInteraction`; overlay extra lines include aggregate state counts as `ai: id:<n> wa:<n> ch:<n> use:<n>`.
- New intent `GameplayIntent::SetMoveTarget { actor_id, point }` was added and is applied only at the safe apply point; `ink:` now includes `mt:<n>`.
- AI interaction attacks use the existing interaction runtime seam by adding `ActiveInteractionKind::{Use,Attack}`. Attack completion/cancel still flows only through `Interaction` system events/intents.
- Movement precedence rule is enforced in AI: it never enqueues `SetMoveTarget` when actor has an active interaction runtime entry or world `OrderState::{Interact,Working}`.
- `player_id` is authoritative: `spawn proto.player` creates an AI-controlled actor but never replaces `GameplayScene.player_id`.
- Scene now auto-restores exactly one authoritative player when missing via `ensure_authoritative_player_exists_if_missing` during tick apply.

## Microticket 37.1 Content Expansion (2026-02-21)
- `assets/base/defs.xml` now includes `proto.npc_chaser`, `proto.npc_dummy`, `proto.stockpile_small`, and `proto.door_dummy` in addition to existing defs.
- Spawn runtime role wiring in `crates/game/src/main.rs` is now tag-driven for debug/intents: `actor` tag spawns with `spawn_actor`, `interactable` tag initializes runtime `Interactable` data.

## Microticket 46.3 Thruport Delivery + Status (2026-02-21)
- Added queueable console command `thruport.status` in engine command registry.
- `thruport.status` output contract is exact and unprefixed: `thruport.status v1 enabled:<0|1> telemetry:<0|1> clients:<u32>`.
- `RemoteConsoleLinePump` now exposes `status_line(telemetry_enabled)`; engine uses this to append status through the same console output path used for remote readback.
- Game-side thruport (`crates/game/src/app/dev_thruport.rs`) now sets accepted client sockets to `TCP_NODELAY` to reduce telemetry line coalescing latency.
- Input injection command acknowledgements remain deterministic (`ok: injected input.<...>`) and are verified through the same remote output forwarding seam.
- `proto.player` now includes the `actor` tag to preserve tag-driven spawn behavior compatibility.
- `proto.door_dummy` currently carries `immediate_use` as metadata only; immediate duration behavior is explicitly deferred (no interaction logic changes in this microticket).

## Ticket 38 Combat Lite Seam (2026-02-21)
- Combat is now resolved in `GameplaySystemId::CombatResolution` by deriving `ApplyDamage` intents only from `InteractionCompleted` events proven to come from `ActiveInteractionKind::Attack` completions in the same tick.
- Scene runtime health store in `crates/game/src/main.rs` is now `HashMap<EntityId, Health>` with `Health { current, max }`; health is initialized for actors only.
- Safe-point intent apply now uses a same-pass pending queue, so follow-up `DespawnEntity` generated by death during `ApplyDamage` is processed deterministically in the same apply invocation.
- `ApplyDamage` behavior:
  - if target missing: invalid target counter increments, no panic
  - if target lacks health: ignored, debug breadcrumb logged, invalid target counter increments
  - if damage applies: emits `EntityDamaged { entity_id, amount_applied }`
  - on transition to zero health: emits `EntityDied { entity_id }` and enqueues same-pass `DespawnEntity`
- Dev probe no longer emits synthetic `EntityDamaged`/`EntityDied`, so `evk dm/dd` now reflects real combat outcomes.

## Ticket 39 Status Effects Seam (2026-02-21)
- Status ids are now string ids (`StatusId(&'static str)`), with shipping status `status.slow`.
- Runtime status storage is scene-owned timed sets: `status_sets_by_entity: HashMap<EntityId, StatusSet>`, where each `StatusSet` contains `ActiveStatus { status_id, remaining_seconds }`.
- `GameplayIntent::AddStatus` now carries `duration_seconds`; reapply uses a single rule: refresh remaining duration.
- `StatusApplied` is emitted only when a status is newly added or refreshed; `StatusExpired` is emitted only when a present status is actually removed.
- `GameplaySystemId::StatusEffects` now runs real ticking logic, decrements durations deterministically, and enqueues `RemoveStatus` intents for expirations.
- Movement modifier is derived, not directly mutated: effective speed uses the product of active status multipliers (`status.slow` currently multiplies by `0.5`; unknown statuses are neutral `1.0`).
- Combat integration now applies `status.slow` on attack-completion-derived damage path (`CombatResolution`), preserving attack-only guard.


## Ticket 40 Main Modularization (2026-02-21)
- `crates/game/src/main.rs` is now a thin composition root (`mod app;`, `build_app`, `run`, `ExitCode`) and is under 300 lines.
- Startup wiring moved to `crates/game/src/app/bootstrap.rs`:
  - owns tracing init, `PROTOGE_ENABLED_MODS` parsing, scene pair construction call, and `LoopConfig` assembly.
  - contract: `build_app() -> AppWiring { config, scene_a, scene_b }`.
- Engine loop invocation moved to `crates/game/src/app/loop_runner.rs`:
  - contract: `run(AppWiring) -> ExitCode`.
  - preserves startup error logging (`startup_failed`) and non-zero exit behavior.
- Gameplay/runtime implementation and tests moved from `crates/game/src/main.rs` into `crates/game/src/app/gameplay.rs` with no behavioral intent changes.
- Gameplay scene factory seam added for bootstrap usage: `build_scene_pair() -> (Box<dyn Scene>, Box<dyn Scene>)`.

## Ticket 41 Dev Thruport Seam (2026-02-21)
- Added `crates/game/src/app/dev_thruport.rs` as a compile-only no-op seam for future thruport/remote console work.
- Defined forward-looking hook contracts only:
  - `ConsoleInputQueueHook::drain_pending_lines`
  - `ConsoleOutputTeeHook::tee_output_line`
  - `InputInjectionHook::inject_input`
- Added placeholder input payload enum `InjectedInput` (`NoOp`, `KeyDown`, `KeyUp`, `MouseMove`).
- Added allocation-free no-op wiring types:
  - `DevThruport`
  - `DevThruportHooks::no_op()`
  - `initialize(DevThruportHooks) -> DevThruport`
- Bootstrap now initializes and carries `AppWiring.dev_thruport`; loop runner explicitly destructures and binds it without affecting runtime behavior.
- No TCP/network/screenshot/input synthesis behavior added; console semantics are unchanged.

## Ticket 42 TCP Thruport Transport (2026-02-21)
- Added localhost-only TCP remote console transport in `crates/game/src/app/dev_thruport.rs`.
- Enablement is env-driven:
  - `PROTOGE_THRUPORT=1` enables listener
  - `PROTOGE_THRUPORT_PORT=<u16>` sets port (default `46001`)
- Bind target is hard-locked to `127.0.0.1:<port>`.
- Transport is non-blocking and line-based (`\n` delimited UTF-8, CRLF tolerated via trailing `\r` strip).
- Invalid UTF-8 lines are dropped with a warning; client/socket errors are non-fatal and do not block the frame loop.
- Added minimal engine runtime hook seam to feed remote lines into the existing console pending-lines path:
  - `RemoteConsoleLinePump`
  - `LoopRuntimeHooks`
  - `run_app_with_hooks(...)`
- Engine redraw flow now polls remote lines once before `ConsoleCommandProcessor::process_pending_lines`, and enqueues via `ConsoleState::enqueue_pending_line`.
- When disabled, no socket is opened and behavior remains equivalent to prior path.

## Ticket 43 Console Output Tee + Remote Readback (2026-02-21)
- Added bounded console output delta buffer in engine console state to expose "new output since last poll".
- `ConsoleState::append_output_line` now tees each output line into:
  - existing visible output history
  - bounded remote-drain buffer (`MAX_NEW_OUTPUT_LINES`).
- Added `ConsoleState::drain_new_output_lines_into(...)` and ensured `clear_output_lines()` clears both output stores.
- Extended engine thruport hook trait `RemoteConsoleLinePump` with default no-op `send_output_lines(&[String])`.
- Engine redraw path now forwards newly appended console output lines to remote hook after command processing/execution.
- Game thruport transport now writes newline-delimited UTF-8 output lines to connected TCP clients via non-blocking writes.
- Readback remains localhost-only transport behavior and does not alter console command semantics or output text.

## Ticket 44 Input Injection Bridge (2026-02-21)
- Added engine queueable debug command family `input.*` (`input.key_down`, `input.key_up`, `input.mouse_move`, `input.mouse_down`, `input.mouse_up`) in `crates/engine/src/app/tools/console_commands.rs`.
- `DebugCommand` now carries `InjectInput { event }` with explicit injected payload types (`InjectedInputEvent`, `InjectedKey`, `InjectedMouseButton`) and strict parse validation/usage errors.
- Input injection apply seam is in `crates/engine/src/app/loop_runner.rs`:
  - commands enqueue injected events into `InputCollector`
  - injected event queue drains only at `InputCollector::snapshot_for_tick` (single stable point per tick)
  - injected action/button states are merged into normal `InputSnapshot` for gameplay reads.
- Console-open suppression remains intact: when console is open, gameplay-facing snapshot is still cleared even if injected events are queued.
- Added disconnect safety reset seam:
  - `RemoteConsoleLinePump::take_disconnect_reset_requested()` default no-op method
  - thruport implementation raises one-shot reset when connected remote clients drop to zero
  - loop marks collector reset and clears held injected keys/buttons on the next snapshot tick.

## Ticket 45 Deterministic Step Control (2026-02-21)
- Added queueable engine simulation-control commands in `crates/engine/src/app/tools/console_commands.rs`:
  - `pause_sim`
  - `resume_sim`
  - `tick <steps>` (`u32`, `> 0`)
- Loop runner in `crates/engine/src/app/loop_runner.rs` now owns sim gating state:
  - `sim_paused: bool`
  - `queued_manual_ticks: u32`
- While paused, frame-time accumulation is disabled and only queued manual ticks advance simulation; rendering/frame pacing still run normally.
- Manual ticks execute through the exact same fixed update path (`snapshot_for_tick` + `scenes.update_active` + apply/switch handling), preserving deterministic behavior and avoiding alternate loops.

## Ticket 46 State Probe Commands (2026-02-21)
- Added queueable console commands `dump.state` and `dump.ai` in `crates/engine/src/app/tools/console_commands.rs`.
- Extended scene debug command routing (`SceneDebugCommand::{DumpState, DumpAi}`) so probe output is scene-owned and goes through existing `execute_debug_command` seam.
- `GameplayScene` now emits versioned, single-line deterministic probe payloads:
  - `dump.state v1 | ...` with player/camera/selection/target/entity/event/intent counters.
  - `dump.ai v1 | ...` with AI state counts and optional deterministic top-5 nearest agent list.
- Probe formatting rules:
  - fixed field order
  - fixed float precision (`{:.2}`)
  - no pointer/internal unstable data beyond existing entity IDs.

## Microticket 46.1 Thruport Frame Telemetry (2026-02-21)
- Added opt-in thruport telemetry push line behind `PROTOGE_THRUPORT_TELEMETRY=1` (requires thruport enabled).
- Telemetry schema (single UTF-8 newline-delimited line, no JSON):  
  `thruport.frame v1 tick:<u64> paused:<0|1> qtick:<u32> ev:<u32> in:<u32> in_bad:<u32>`
- Emission point is engine loop fixed-step execution: exactly one telemetry line per executed fixed tick on the normal update path (includes manual `tick <N>` execution path).
- `tick` is monotonic loop-owned counter; `paused`/`qtick` come from existing sim gating state; `ev`/`in`/`in_bad` are derived from active-scene debug snapshot extra lines (`ev:`, `in:`, `in_bad:`), with zero fallback when unavailable.
- Existing console `ok:`/`error:` output lines remain unchanged and unprefixed.

## Microticket 46.2 Sync Barrier + Reset Semantics (2026-02-21)
- Added queueable engine console command `sync` that appends exactly `ok: sync` in queued command order, acting as a command-processing barrier for remote automation transcripts.
- Tightened thruport disconnect reset semantics in game transport: one-shot reset flag is now set whenever any TCP client is removed (EOF/read error/write error), not only when all clients disconnect.
- Hardened `dump.state v1` format contract tests in gameplay:
  - fixed key order checks (`player -> cam -> sel -> tgt -> cnt -> ev -> evk -> in -> ink -> in_bad`)
  - required-key presence checks for populated and empty-world cases
  - fixed two-decimal precision assertions for player/camera coordinates.
