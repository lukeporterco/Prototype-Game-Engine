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

- Deprecated in-place detailed notes (Module Boundaries + Tickets 33-47) were moved to `CODEXNOTES_ARCHIVE.md` on 2026-02-23.

- Ticket 48 (2026-02-23): `EntityDef` now carries optional gameplay knobs in content runtime data (`health_max`, `base_damage`, `aggro_radius`, `attack_range`, `attack_cooldown_seconds`) through compile/pack/database/archetype as `Option`.
- Ticket 48: gameplay runtime defaults for those knobs are centralized in `GameplayScene::effective_combat_ai_params` in `crates/game/src/app/gameplay.rs` to preserve legacy behavior when fields are omitted.
- Ticket 48: attacker damage source is `GameplayScene.damage_by_entity: HashMap<EntityId, u32>`; populated during `SpawnByArchetypeId`, consumed by `CombatResolution`, and cleaned on sync/reset/despawn.
- Status model reminder: statuses use `StatusId(&'static str)` and shipping slow status id is `status.slow`.

## Module Boundaries and Ownership
### A. Module map
#### Core
- Core primitives and IDs shared across engine/game crates.
#### App/Loop
- Frame loop, fixed timestep cadence, runtime metrics, and scene tick orchestration.
#### SceneMachine and Scene
- Active/inactive scene ownership, scene switching, load/reset boundaries.
#### World (SceneWorld and runtime state)
- Entity storage, transforms, runtime tags/components, spawn/despawn application.
#### Rendering
- Camera transforms, world-to-screen math, sprite/placeholder draw path.
#### Assets and Content Pipeline
- XML discovery/compile, cached content packs, runtime `DefDatabase`.
#### Input
- Per-frame input sampling and action mapping.
#### Tools (Overlay, Console)
- Debug overlay text, console command routing, diagnostics surfaces.
#### Placeholders (Physics, Audio, Scripting seam)
- Reserved seams only; no heavy implementation yet.
### B. Ownership rules
- Engine owns generic runtime/data flow; game crate owns gameplay rules/defs consumption.
- Runtime sim does not parse XML; content is consumed via compiled `DefDatabase`.
- Keep systems deterministic and avoid broad cross-module coupling.
### C. Seam invariants
- Scene/game logic may read defs but must not mutate content database contracts.
- Def defaults for Ticket 48 gameplay knobs are centralized in `GameplayScene` helper.
- Simulation intent ordering remains `InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup`.

- Ticket 49 (2026-02-23): `ContentCompileError` now includes optional structured context fields `def_name` and `field_name` so gameplay tuning validation failures are deterministic and testable.
- Ticket 49: gameplay tuning validation fixture added at `docs/fixtures/content_pipeline_v1/fail_09_invalid_gameplay_field/badgameplay/defs.xml`; pipeline tests assert `InvalidValue` plus `mod_id`/`def_name`/`field_name`.
- Ticket 50 (2026-02-23): regression coverage for knobbed sample content is anchored in `crates/engine/src/content/pipeline.rs` test `base_defs_load_proto_npc_chaser_with_expected_tuning_fields` (loads base defs and asserts parsed tuning values for `proto.npc_chaser`).
- Ticket 50: gameplay smoke coverage for tuned chaser + shipping slow lifecycle is in `crates/game/src/app/gameplay.rs` test `proto_npc_chaser_attack_applies_slow_then_slow_expires`.
- Ticket 51.1 (2026-02-23): queueable command `thruport.telemetry <on|off>` added in `crates/engine/src/app/tools/console_commands.rs`; runtime loop telemetry state is mutable per-session with explicit schema output `ok: thruport.telemetry v1 enabled:<0|1>`.
- Ticket 51.1: remote TCP wire contract now applies last-mile channel prefixes in `crates/game/src/app/dev_thruport.rs` (`C ` control, `T ` telemetry), and sends per-client ready line on accept: `C thruport.ready v1 port:<u16>`.
- Ticket 51.2 (2026-02-23): repo-owned CLI harness added as workspace crate `crates/thruport_cli` (`cargo build -p thruport_cli`) for deterministic thruport automation without shell socket scripts.
- Ticket 51.2: CLI contract lives in `crates/thruport_cli/src/lib.rs` and `crates/thruport_cli/src/main.rs` with subcommands `wait-ready`, `send`, `script [--barrier]`, `barrier`; default output is control-only and strips `C/T` tags, `--include-telemetry` includes telemetry payloads.
- Ticket 51.3 (2026-02-23): queueable command `scenario.setup <scenario_id>` added to engine parser/routing (`crates/engine/src/app/tools/console_commands.rs`, `crates/engine/src/app/loop_runner.rs`) and scene debug API (`crates/engine/src/app/scene.rs`).
- Ticket 51.3: gameplay scene owns `combat_chaser` setup in `crates/game/src/app/gameplay.rs` via safe-point intents only; contract line is `ok: scenario.setup combat_chaser player:<id> chaser:<id> dummy:<id>`.
- Ticket 51.3: deterministic overwrite rule is scene-local slot replacement (previous scenario chaser/dummy and current authoritative player are despawned, then player/chaser/dummy respawn at fixed coordinates and player is re-selected).
- Ticket 51.4 (2026-02-23): added repo tooling helper `scripts/test-helper.ps1` for deterministic test discovery and single-test execution.
- Ticket 51.4: helper supports `-Mode list` and `-Mode run-one` for packages `engine|game|thruport_cli`; `run-one` resolves regex to exactly one canonical test name and executes with `cargo test -p <pkg> <name> -- --exact`.
- Ticket 51.4: workflow docs added in `docs/test_helper.md` and linked from `README.md`.
- Ticket 52 (2026-02-23): `crates/game/src/app/gameplay.rs` was structurally decomposed into `crates/game/src/app/gameplay/` chunk files (`mod.rs`, `types.rs`, `systems.rs`, `scene_state.rs`, `scene_impl.rs`, `util.rs`, `tests.rs`) using include-based composition to preserve runtime behavior and private-access semantics.
- Ticket 52: external gameplay entrypoint contract is unchanged (`crate::app::gameplay::build_scene_pair`), and system order constants/intent pipeline behavior remain intact.
- Ticket 53 (2026-02-23): `thruport_cli send` now uses an internal `sync` completion boundary in `crates/thruport_cli/src/lib.rs` and suppresses the internal `ok: sync` line from default output.
- Ticket 53: `thruport_cli` fallback behavior uses quiet-window completion only when internal sync is unavailable; CLI now exposes `--quiet-ms` (default `250`) in `crates/thruport_cli/src/main.rs` and `docs/thruport_cli.md`.
- Ticket 53: `script --barrier` contract remains one explicit end-of-script barrier (no added per-command barriers).
