# V2 Backlog (Canonical)

This is the single canonical backlog for deferred V2 ideas captured during ticket work.

How to use this doc:
- When a ticket includes ROADMAP `V2` bullets, append each bullet as a separate entry under exactly one module section below.
- Use the strict template exactly as written (field names, order, and headings).
- If a ticket says `V2: None`, do not change this file.

## Strict Entry Template (Copy/Paste)
```md
### [Short title]
- Date: YYYY-MM-DD
- Source: <ticket id or doc link>
- Area: <Core|App/Loop|Scene|Rendering|Assets|Input|Tools|Physics placeholder|Audio placeholder|Scripting seam|Build/CI>
- Summary: <one concise statement of the deferred V2 change>
- Rationale: <why this should exist>
- Dependencies: <none or required prior work>
- Risks: <none or key risk(s)>
- Cut: <what is explicitly out of scope right now>
```

## Core
### [Deterministic startup path diagnostics]
- Date: 2026-03-01
- Source: Ticket 0
- Area: Core
- Summary: Add a deterministic startup-path diagnostic surface that reports resolved root/base/mods/cache and root-marker selection reason.
- Rationale: Ticket 0 introduced deterministic path resolution and this V2 makes path decisions externally auditable for support and automation. Deterministic verification: `cargo test -p engine startup_paths_diagnostics_v1` => `test result: ok.` and emitted probe line pattern `paths.resolve v1 root:<...> base:<...> mods:<...> cache:<...> marker:<...>`.
- Dependencies: Existing `resolve_app_paths` path-resolution seam in `crates/engine/src/lib.rs`.
- Risks: Diagnostic schema drift if fields are not versioned.
- Cut: No runtime path mutation.

### [Content cache repair report artifact]
- Date: 2026-03-01
- Source: Ticket 9
- Area: Core
- Summary: Emit a deterministic cache repair report artifact listing rebuilt packs and causes.
- Rationale: Ticket 9 introduced build/load orchestration and this V2 adds reproducible forensics for rebuild decisions. Deterministic verification: `cargo test -p engine content_cache_repair_report_v1` => `test result: ok.` and report lines sorted by mod id with pattern `<mod_id> reason:<...>`.
- Dependencies: Existing planner/pipeline cache decision model.
- Risks: Report noise if rebuild reasons are too granular.
- Cut: No external telemetry upload.

### [DefDatabase tag index]
- Date: 2026-03-01
- Source: Ticket 8
- Area: Core
- Summary: Add a deterministic tag->entity-def-id index in DefDatabase for fast runtime lookups.
- Rationale: Ticket 8 established DefDatabase runtime contracts; this V2 removes repeated tag scans and improves deterministic query performance. Deterministic verification: `cargo test -p engine def_database_tag_index_lookup_v1` => `test result: ok.` and stable sorted id list for repeated lookups.
- Dependencies: Existing compiled-content->DefDatabase load path.
- Risks: Ordering bugs if ids are not sorted canonically in index output.
- Cut: No fuzzy tag search.

### [Save-id audit preflight]
- Date: 2026-03-01
- Source: Ticket 21
- Area: Core
- Summary: Add deterministic pre-save audit for missing/duplicate/dangling save-id references.
- Rationale: Ticket 21 made stable save IDs first-class; this V2 adds explicit integrity checks before write. Deterministic verification: `cargo test -p game save_id_audit_v1` => `test result: ok.` with probe pattern `save_id.audit v1 missing:<u32> duplicate:<u32> dangling:<u32>`.
- Dependencies: Existing save-id synchronization and validation-first save/load flow.
- Risks: False positives if transient-only references are audited as persisted references.
- Cut: No auto-repair mode.

### [EntityDef cross-field validation]
- Date: 2026-03-01
- Source: Ticket 48
- Area: Core
- Summary: Add deterministic cross-field validation rules for gameplay knobs and explicit default surfacing.
- Rationale: Ticket 48 added gameplay knobs to content defs; this V2 hardens correctness for invalid combinations and default behavior visibility. Deterministic verification: `cargo test -p engine entitydef_cross_field_validation_v1` => `test result: ok.` with stable error token pattern `error: invalid gameplay field`.
- Dependencies: Existing content compiler validation and error context path.
- Risks: Over-constraining tuning workflows if rules are too strict.
- Cut: No balancing heuristics.

### [Structured compile error ordering and JSON dump]
- Date: 2026-03-01
- Source: Ticket 49
- Area: Core
- Summary: Add stable multi-error ordering and optional JSON compile-error dump for tooling.
- Rationale: Ticket 49 introduced structured compile context; this V2 standardizes deterministic multi-error emission for automation consumers. Deterministic verification: `cargo test -p engine compile_error_order_and_json_dump_v1` => `test result: ok.` and repeated runs emit identical ordered error keys.
- Dependencies: Existing `ContentCompileError` structured context model.
- Risks: Tooling breakage if JSON field names are changed without versioning.
- Cut: No non-fatal compile continuation.

### [Content regression fixture expansion]
- Date: 2026-03-01
- Source: Ticket 50
- Area: Core
- Summary: Expand content pipeline fixtures for gameplay-knob override merges and pack-bump coverage.
- Rationale: Ticket 50 is regression coverage work; this V2 broadens deterministic fixture coverage for newly added knobs and format transitions. Deterministic verification: `cargo test -p engine content_pipeline_regression_fixtures_v2` => `test result: ok.`.
- Dependencies: Existing fixture harness and pack-versioned test paths.
- Risks: Fixture maintenance overhead.
- Cut: No external fixture generator tooling.

## App/Loop
### [Runtime TPS setter]
- Date: 2026-03-01
- Source: Ticket 1
- Area: App/Loop
- Summary: Add debug runtime TPS override command with explicit clamp and effective-value output.
- Rationale: Ticket 1 established fixed-step pacing; this V2 adds a deterministic runtime tuning seam without rebuilds. Deterministic verification: `thruport_cli --port 46001 send "tps.set 30"` => `ok: tps.set v1 requested:30 effective:30 clamp:none`.
- Dependencies: Adds new console command(s): `tps.set`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Misuse may hide pacing regressions during ad-hoc testing.
- Cut: No persisted TPS override file.

### [Telemetry schema probe]
- Date: 2026-03-01
- Source: Microticket 46.1
- Area: App/Loop
- Summary: Add explicit telemetry-schema probe command for runtime capability discovery.
- Rationale: Microticket 46.1 introduced telemetry frames; this V2 gives clients a deterministic compatibility check before parsing. Deterministic verification: `thruport_cli --port 46001 send telemetry.schema` => `ok: telemetry.schema v1 frame_schema:thruport.frame.v1`.
- Dependencies: Adds new console command(s): `telemetry.schema`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Schema drift if reported identifier and emitted frame format diverge.
- Cut: No schema negotiation handshake.

### [Deterministic tick-until control]
- Date: 2026-03-01
- Source: Ticket 45
- Area: App/Loop
- Summary: Add deterministic `tick.until <u64>` control with explicit remaining count token.
- Rationale: Ticket 45 added manual stepping; this V2 reduces script-side loops while preserving the same fixed update path. Deterministic verification: `thruport_cli --port 46001 send "tick.until 120"` => `ok: tick.until v1 target:120 remaining:120`.
- Dependencies: Adds new console command(s): `tick.until`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Ambiguity with existing `tick <steps>` queue semantics if precedence is undocumented.
- Cut: No wall-clock delay mode.

### [Thruport status field extension]
- Date: 2026-03-01
- Source: Microticket 46.3
- Area: App/Loop
- Summary: Extend `thruport.status` with deterministic queue-depth and dropped-line counters for control and telemetry categories.
- Rationale: Microticket 46.3 owns status delivery/observability and this V2 expands the single-line contract with actionable transport pressure fields. Deterministic verification: `thruport_cli --port 46001 send thruport.status` => `thruport.status v2 enabled:<0|1> telemetry:<0|1> clients:<u32> qdepth:<u32> drop_ctl:<u32> drop_tel:<u32>`.
- Dependencies: Changes existing console command(s): `thruport.status`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Backward-compat impact for scripts pinned to v1 status shape.
- Cut: No per-client verbose status dump.

### [Console output readback backpressure policy]
- Date: 2026-03-01
- Source: Ticket 43
- Area: App/Loop
- Summary: Add explicit output-readback backpressure policy and observable readback drop counters for remote console output tee.
- Rationale: Ticket 43 added output tee + remote readback; this V2 makes readback pressure behavior deterministic and inspectable without overlapping `thruport.status` field ownership. Deterministic verification: `thruport_cli --port 46001 send readback.status` => `ok: readback.status v1 policy:<drop_oldest|drop_newest|block_never> out_qdepth:<u32> out_drop:<u32>`.
- Dependencies: Adds new console command(s): `readback.status`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Counter semantics confusion if reset boundaries are not explicit.
- Cut: No change to core command execution semantics.

## Scene

### Action state interact/carry emission
- Date: 2026-02-28
- Source: Ticket 62.1
- Area: Scene
- Summary: Emit `Interact` and `Carry` action visuals for one interactable workflow.
- Rationale: Extend the MVP Idle/Walk visual contract so renderer can reflect core interaction intent.
- Dependencies: Stable interaction-state to action-state mapping for player workflow.
- Risks: Visual-state drift if action emission is not aligned with deterministic gameplay tick boundaries.
- Cut: No blend trees, IK, ragdolls, or physics-driven animation.

### Visual sandbox second interactable lane
- Date: 2026-02-28
- Source: Ticket 62.4
- Area: Scene
- Summary: Add a second deterministic interactable lane in `visual_sandbox` with role-stable payload semantics.
- Rationale: Expand demo coverage for interaction-state visuals without changing command schema or scenario id.
- Dependencies: Stable visual-sandbox spawn ordering and interaction target resolution by save-id.
- Risks: Scenario clutter can reduce readability if overlap/layout is not kept intentional.
- Cut: No new console commands and no scenario payload schema changes.

### Visual sandbox deterministic hit-kick trigger
- Date: 2026-02-28
- Source: Ticket 62.4
- Area: Scene
- Summary: Add a deterministic demo trigger path that forces `Hit` action visual state in `visual_sandbox`.
- Rationale: Ensure hit-kick polish can be validated visually in a repeatable scene.
- Dependencies: Existing renderer-side `Hit` procedural support and deterministic sandbox state rules.
- Risks: Trigger timing may conflict with ongoing interaction states if priority rules are unclear.
- Cut: No combat-system redesign and no non-deterministic/random trigger logic.

### Multi-select settlers move order
- Date: 2026-02-28
- Source: Ticket 63
- Area: Scene
- Summary: Add box-select and issue-move behavior that sends one move order to all selected settlers.
- Rationale: Improves colony-control ergonomics beyond single-pawn ordering.
- Dependencies: Stable per-entity pawn-role classification and deterministic group-order dispatch rules.
- Risks: Selection ordering and per-entity move target fan-out could drift determinism if processing order is not fixed.
- Cut: No formations, squad tactics, or shared-path steering.

### Diagonal tile movement for settlers
- Date: 2026-03-01
- Source: Ticket 64
- Area: Scene
- Summary: Extend settler tile pathfinding from 4-way to diagonal movement with explicit corner-cutting rules.
- Rationale: Produces shorter, more natural routes while preserving deterministic navigation behavior.
- Dependencies: Stable tile-grid A* seam and clear blocked-corner policy decisions.
- Risks: Corner-cutting edge cases can introduce path acceptance inconsistencies if rules are not strict.
- Cut: No navmesh conversion and no crowd steering.

### Tilemap epoch-triggered lightweight re-path
- Date: 2026-03-01
- Source: Ticket 64
- Area: Scene
- Summary: Add tilemap revision/epoch tracking so settlers can re-path only when relevant tile passability changes.
- Rationale: Avoids stale routes after terrain edits without per-frame path recomputation.
- Dependencies: Tilemap change signaling contract available to gameplay scene state.
- Risks: Incorrect epoch propagation can leave actors on stale paths or trigger unnecessary recomputes.
- Cut: No dynamic obstacle avoidance for moving entities.

### Settler open-job auto-pick
- Date: 2026-03-01
- Source: Ticket 65
- Area: Scene
- Summary: Let idle settlers scan deterministic open jobs and claim nearest unreserved work automatically.
- Rationale: Removes per-settler micro-ordering overhead once first-class jobs exist.
- Dependencies: Stable JobBoard lifecycle, reservation semantics, and deterministic distance tie-break policy.
- Risks: Non-deterministic claim order if actor/job iteration is not explicitly fixed.
- Cut: No global colony scheduler and no cross-map batching.

### Job priority and reservation timeout
- Date: 2026-03-01
- Source: Ticket 65
- Area: Scene
- Summary: Add numeric job priorities and reservation timeout recovery for stuck claims.
- Rationale: Improves responsiveness and prevents dead reservations in longer simulations.
- Dependencies: Baseline first-class JobBoard assignment and per-job state transitions.
- Risks: Priority starvation and timeout churn if policy is not bounded and deterministic.
- Cut: No UI work planner and no skill/need-based weighting.

### [Debug snapshot block versioning]
- Date: 2026-03-01
- Source: Ticket 14
- Area: Scene
- Summary: Version logical blocks within `dump.state` so consumers can migrate safely as fields evolve.
- Rationale: Ticket 14 established inspect/debug snapshot surfaces; this V2 adds explicit block-level compatibility markers. Deterministic verification: `thruport_cli --port 46001 send dump.state` => output includes stable tokens `inspect_v2` `counts_v1` `intents_v1` in fixed order.
- Dependencies: Changes existing console command(s): `dump.state`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Consumer breakage if block tags change without version bumps.
- Cut: No binary probe format.

### [Soft reset mode]
- Date: 2026-03-01
- Source: Ticket 14.5
- Area: Scene
- Summary: Add `soft` reset mode that clears gameplay entities but preserves camera/tilemap.
- Rationale: Ticket 14.5 introduced persistent scenes with hard reset; this V2 adds a deterministic faster iteration reset path. Deterministic verification: `thruport_cli --port 46001 send "reset_scene soft"` => `ok: scene reset soft v1 preserved:camera,tilemap`.
- Dependencies: Changes existing console command(s): `reset_scene`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: State leakage if non-entity runtime stores are not explicitly handled.
- Cut: No subsystem-selective reset matrix.

### [Save validation command]
- Date: 2026-03-01
- Source: Ticket 30
- Area: Scene
- Summary: Add deterministic named-save validation command returning first failure in runtime load format.
- Rationale: Ticket 30 focused save/load diagnostics and this V2 adds scriptable preflight validation without full restore. Deterministic verification: `thruport_cli --port 46001 send "save.validate autosave"` => `ok: save.validate v1 slot:autosave` or `error: save.validate v1 slot:autosave field:<...>`.
- Dependencies: Adds new console command(s): `save.validate`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Divergence from runtime loader error formatting if paths fork.
- Cut: No multi-slot batch validation.

### [Jobs/nav probe commands]
- Date: 2026-03-01
- Source: Ticket 46
- Area: Scene
- Summary: Add deterministic versioned probes for job-state and nav-state internals.
- Rationale: Ticket 46 added state probe commands and this V2 extends probe coverage for newer gameplay systems without pixel inspection. Deterministic verification: `thruport_cli --port 46001 send dump.jobs` => `ok: dump.jobs v1 ...` and `thruport_cli --port 46001 send dump.nav` => `ok: dump.nav v1 ...`.
- Dependencies: Adds new console command(s): `dump.jobs`, `dump.nav`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Probe payload growth affecting readability and throughput.
- Cut: No full per-entity history replay.

### [Selection by save-id and nearest tag]
- Date: 2026-03-01
- Source: Ticket 47
- Area: Scene
- Summary: Add deterministic selection helpers by stable save-id and nearest tag.
- Rationale: Ticket 47 established deterministic select/order commands; this V2 reduces brittle runtime-id coupling in scripts. Deterministic verification: `thruport_cli --port 46001 send "select.save_id 1"` => `ok: selected entity <u64> save_id:1`.
- Dependencies: Adds new console command(s): `select.save_id`, `select.nearest_tag`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Nearest-tag ties need fixed tie-break ordering to preserve determinism.
- Cut: No fuzzy tag matching.

### [Floor-authored default + targeting probe]
- Date: 2026-03-01
- Source: Ticket 56
- Area: Scene
- Summary: Add authored per-def default floor and floor-restricted interaction targeting rules.
- Rationale: Ticket 56 introduced floor layering and `floor.set`; this V2 makes default floor/target restrictions content-driven and externally inspectable. Deterministic verification: `thruport_cli --port 46001 send dump.state` => output contains stable tokens `floor_default:<rooftop|main|basement>` and `target_floor_filter:<...>`.
- Dependencies: Changes existing console command(s): `dump.state`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Cross-floor interaction regressions if filters are only partially applied.
- Cut: No multi-floor pathfinding rewrite.

## Rendering

### Hand/tool anchor emission for UseTool visuals
- Date: 2026-02-28
- Source: Ticket 62.2
- Area: Rendering
- Summary: Emit and consume `hand`/`tool` sprite anchors to support `UseTool` held/tool attachment visuals.
- Rationale: Extend the carry-anchor MVP into practical tool-use presentation without changing simulation authority.
- Dependencies: Stable `UseTool` action visual emission and authored sprite anchor coverage.
- Risks: Anchor naming/authoring drift across sprite sets can cause visual misalignment.
- Cut: No IK, bone rigs, blend trees, or per-pixel deformation systems.

### Procedural recoil from deterministic seeds
- Date: 2026-02-28
- Source: Ticket 62.3
- Area: Rendering
- Summary: Add renderer-only recoil offsets driven by deterministic per-entity seeds and fixed-tick phase.
- Rationale: Improve action feedback while preserving simulation authority and FPS-independent visual determinism.
- Dependencies: Stable per-entity action-state/action-params visual payload and fixed-tick phase source.
- Risks: Overtuned amplitudes can reduce readability or create perceived jitter near micro-grid boundaries.
- Cut: No simulation-side recoil forces, IK, or skeletal animation systems.

### Deterministic light flicker polish
- Date: 2026-02-28
- Source: Ticket 62.3
- Area: Rendering
- Summary: Add renderer-only light flicker modulation using deterministic per-entity seeds.
- Rationale: Increase scene liveliness with repeatable visual variation that stays independent from simulation logic.
- Dependencies: Existing renderer-only procedural layer and deterministic tick-derived phase.
- Risks: Excessive modulation can become distracting and conflict with clarity on low-end displays.
- Cut: No dynamic lighting pipeline rewrite, no physics-driven effects, and no gameplay visibility logic changes.

### [TileDef-driven tile sprite mapping]
- Date: 2026-03-01
- Source: Ticket 18
- Area: Rendering
- Summary: Move tile-id->sprite-key mapping from renderer constants to compiled TileDef content data.
- Rationale: Ticket 18 established tilemap runtime rendering and engine-owned tile constants; this V2 shifts mapping ownership to content for extensibility. Deterministic verification: `cargo test -p engine tiledef_sprite_mapping_v1` => `test result: ok.` and deterministic mapping assertions by tile id.
- Dependencies: Blocked on TileDef schema definition, compiler parse/validate support, pack encode/decode wiring, DefDatabase runtime access, and renderer lookup integration.
- Risks: Undefined fallback semantics for missing tile ids.
- Cut: No material/lighting pipeline changes.

### [Renderable z-bias field]
- Date: 2026-03-01
- Source: Ticket 22
- Area: Rendering
- Summary: Add authored small-int z-bias/layer field affecting render overlap key only.
- Rationale: Ticket 22 formalized renderable attribute parsing; this V2 adds deterministic artist control over overlap order without changing simulation transforms. Deterministic verification: `cargo test -p engine render_sort_z_bias_v1` => `test result: ok.` with unchanged pick winners.
- Dependencies: Renderable schema + pack/runtime field wiring.
- Risks: Overuse can make scene depth perception confusing.
- Cut: No floating-point depth model.

### [Micro-grid snap opt-out]
- Date: 2026-03-01
- Source: Ticket 55
- Area: Rendering
- Summary: Add per-renderable snap opt-out while keeping snap-on default.
- Rationale: Ticket 55 established micro-grid snap policy and this V2 introduces controlled exceptions for special visuals. Deterministic verification: `cargo test -p engine snap_policy_opt_out_v1` => `test result: ok.` with deterministic pixel-coordinate assertions for snapped vs unsnapped cases.
- Dependencies: Renderable runtime flag and renderer transform branch.
- Risks: Visual inconsistency if opt-out usage expands without guidance.
- Cut: No dynamic sub-pixel animation system.

### [Missing sprite audit command]
- Date: 2026-03-01
- Source: Ticket 58.1
- Area: Rendering
- Summary: Add deterministic command to report sorted unique missing sprite keys observed in-session.
- Rationale: Ticket 58.1 covered visual-test sprite warnings and this V2 makes missing-asset diagnostics scriptable. Deterministic verification: `thruport_cli --port 46001 send assets.audit_missing_sprites` => `ok: assets.audit_missing_sprites v1 count:<u32> entries:<...>` with sorted entries.
- Dependencies: Adds new console command(s): `assets.audit_missing_sprites`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Session accumulator growth if unbounded.
- Cut: No automatic path correction.

### [Sprite pivot offset]
- Date: 2026-03-01
- Source: Ticket 59
- Area: Rendering
- Summary: Add authored integer sprite pivot offsets for scale-aware placement control.
- Rationale: Ticket 59 added sprite pixelScale behavior; this V2 avoids forced center-on-tile alignment for all assets. Deterministic verification: `cargo test -p engine sprite_pivot_offset_v1` => `test result: ok.` with deterministic screen-position assertions.
- Dependencies: Sprite schema field + pack/runtime + renderer wiring.
- Risks: Asset pivot inconsistency causing alignment drift.
- Cut: No per-frame animated pivots.

## Assets

## Input

## Tools

### Command Palette multi-step macros
- Date: 2026-02-28
- Source: Microticket 61.3
- Area: Tools
- Summary: Add user-defined command palette macros that execute multiple console commands in sequence.
- Rationale: Improve iteration speed for repeated debug/setup workflows without changing core command routing.
- Dependencies: Finalize command palette preset persistence format and macro execution safety rules.
- Risks: Hidden ordering assumptions could reduce determinism if macros are used without explicit barriers.
- Cut: No macro authoring UI or CI automation in current scope.

### Overlay pawn role indicator
- Date: 2026-02-28
- Source: Ticket 63
- Area: Tools
- Summary: Show selected actor control role in overlay inspect text (`PlayerPawn`, `Settler`, `Npc`).
- Rationale: Makes control semantics visible during testing without opening code/logs.
- Dependencies: Gameplay exposes stable role value for selected entity in debug snapshot path.
- Risks: Overlay text bloat can reduce readability on low-resolution displays.
- Cut: No new HUD panels or interactive UI widgets.

### [Overlay page model]
- Date: 2026-03-01
- Source: Ticket 5
- Area: Tools
- Summary: Add deterministic overlay page cycling to keep default overlay concise.
- Rationale: Ticket 5 introduced overlay v0; this V2 preserves readability while exposing deeper diagnostics on demand. Deterministic verification: `thruport_cli --port 46001 send overlay.page` => `ok: overlay.page v1 current:<core|inspect|perf>` with deterministic cycle order.
- Dependencies: Adds new console command(s): `overlay.page`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Hidden critical data if page defaults are poorly chosen.
- Cut: No interactive panel layout editor.

### [Per-system timing in state probe]
- Date: 2026-03-01
- Source: Ticket 25
- Area: Tools
- Summary: Add compact per-system timing tokens to deterministic probe output.
- Rationale: Ticket 25 introduced perf counters and this V2 extends observability to subsystem hotspots. Deterministic verification: `thruport_cli --port 46001 send dump.state` => output includes `sys_perf:` with stable system ordering.
- Dependencies: Changes existing console command(s): `dump.state`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Probe overhead can perturb measurements.
- Cut: No full timeline profiler export.

### [Budget warning latch fields]
- Date: 2026-03-01
- Source: Ticket 26
- Area: Tools
- Summary: Expose deterministic budget-warning latch state in probe output.
- Rationale: Ticket 26 added soft perf budgets and warning gates; this V2 makes warning state machine script-assertable. Deterministic verification: `thruport_cli --port 46001 send dump.state` => output includes `budget_warn_sim:<0|1>` and `budget_warn_ren:<0|1>`.
- Dependencies: Changes existing console command(s): `dump.state`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: False confidence if latch reset semantics are undocumented.
- Cut: No auto-throttle actions.

### [Schema-driven help text]
- Date: 2026-03-01
- Source: Ticket 32.2
- Area: Tools
- Summary: Generate help output from one authoritative command schema source.
- Rationale: Ticket 32.2 parser/registry work benefits from removing manual help drift. Deterministic verification: `thruport_cli --port 46001 send help` => stable ordered output with typed usage tokens from schema.
- Dependencies: Changes existing console command(s): `help`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Schema migration can temporarily desync parser and formatter.
- Cut: No localization/i18n.

### [Queueable command trace mode]
- Date: 2026-03-01
- Source: Ticket 32.3
- Area: Tools
- Summary: Add trace toggle emitting stable execution token per queueable command.
- Rationale: Ticket 32.3 established queueable routing semantics and this V2 improves automation debugging correlation. Deterministic verification: `thruport_cli --port 46001 send "command.trace on"` => `ok: command.trace v1 enabled:1` and subsequent queueable outputs include `trace:<token>`.
- Dependencies: Adds new console command(s): `command.trace`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Output noise if left enabled in broad scripts.
- Cut: No structured JSON trace stream.

### [Overlay scale presets]
- Date: 2026-03-01
- Source: Ticket 61.2
- Area: Tools
- Summary: Add fixed overlay scale presets for font size/padding with deterministic preset ids.
- Rationale: Ticket 61.2 improved overlay readability and this V2 formalizes accessibility sizing without arbitrary free-form layout drift. Deterministic verification: `thruport_cli --port 46001 send "overlay.scale medium"` => `ok: overlay.scale v1 preset:medium font:<u32> pad:<u32>`.
- Dependencies: Adds new console command(s): `overlay.scale`. Requires updating docs/CONSOLE_COMMANDS.md in the same work unit.
- Risks: Clipping regressions at low resolutions if bounds are not recomputed.
- Cut: No continuous slider scaling.

## Physics placeholder

## Audio placeholder

## Scripting seam

## Build/CI
### [Golden transcript fixtures]
- Date: 2026-03-01
- Source: Ticket 29
- Area: Build/CI
- Summary: Add golden transcript fixtures driven by thruport_cli scripts.
- Rationale: Ticket 29 determinism harness intent is strengthened by byte-stable transcript assertions in CI. Deterministic verification: `cargo test -p thruport_cli transcript_golden_v1` => `test result: ok.`.
- Dependencies: Existing CLI script/barrier execution path and deterministic probe contracts.
- Risks: Fixture brittleness if text contracts are changed frequently.
- Cut: No screenshot-based validation.

### [Dev thruport transport contract tests]
- Date: 2026-03-01
- Source: Ticket 41
- Area: Build/CI
- Summary: Add explicit transport contract tests for line delivery/buffering/ordering guarantees.
- Rationale: Ticket 41 introduced dev thruport seam contracts and this V2 locks those guarantees with regression tests. Deterministic verification: `cargo test -p game dev_thruport_transport_contract_v1` => `test result: ok.`.
- Dependencies: Existing `dev_thruport` hook and test seam.
- Risks: Under-specified contract could still permit ambiguous behavior.
- Cut: No alternative transport implementation.

### [Cross-platform test helper parity]
- Date: 2026-03-01
- Source: Ticket 51.4
- Area: Build/CI
- Summary: Add bash test-helper equivalent and deterministic tagged run-suite mode.
- Rationale: Ticket 51.4 test helper workflow is improved by platform parity for deterministic test selection/execution. Deterministic verification: `bash scripts/test-helper.sh --mode run-suite --tag smoke` => exit `0` with stable matched-test list prefix.
- Dependencies: Existing PowerShell helper behavior contract.
- Risks: Drift between PowerShell and bash argument semantics.
- Cut: No external test orchestrator integration.
