# Console Commands (Developer Reference)
Last updated: 2026-02-28. Covers: Tickets 0-56.

Status: `Ticket 32.3` implements routing/execution for queueable commands.

## Console Controls

- Toggle console: `` ` `` (Backquote)
- While open:
- Type text into prompt
- `Backspace` deletes one character
- `Enter` submits current line
- `Up` / `Down` browse submission history
- `Escape` closes console and clears current input line

## Dev Command Palette (Ticket 61.1)

- Scope: debug/dev UI convenience only (`cfg!(debug_assertions)` builds).
- Purpose: emits plain command strings through the same pending-line parser path used by typed console input.
- Layer: Engine tools UI front-end; does not bypass command parsing/routing.
- Behavior:
- Non-placement buttons submit one command immediately.
- Spawn placement buttons arm `spawn <def_name>` for one world click.
- Armed left-click in world emits `spawn <def_name> <x> <y>` (2-decimal formatting) and then disarms.
- Armed right-click cancels without emitting a spawn command.
- Clicking inside the palette panel while armed does not place in world.
- Spawn button population:
- Uses current runtime `DefDatabase` entity defs from active world.
- Does not hardcode missing defs (`proto.worker`, `proto.wall`, `proto.crate` are omitted unless present in loaded content).

## Command Handling Model

- Local actions (handled immediately, never queued):
- `help`
- `clear`
- `echo`
- Queueable actions (parsed to `DebugCommand`, routed and executed in the engine loop):
- `reset_scene`
- `switch_scene`
- `quit`
- `despawn`
- `spawn`
- `select`
- `order.move`
- `order.interact`
- `dump.state`
- `dump.ai`
- `scenario.setup`
- `floor.set`
- `pause_sim`
- `resume_sim`
- `tick`
- `thruport.status`
- `thruport.telemetry`
- `input.key_down`
- `input.key_up`
- `input.mouse_move`
- `input.mouse_down`
- `input.mouse_up`
- Queueable command output format:
- Success: `ok: ...`
- Failure: `error: ...`
- No `queued:` success lines are emitted for queueable commands.

## Local Commands (Immediate)

### help
- Layer: Engine console processor
- Description: Lists available commands in registry registration order.
- Syntax: `help`
- Example:
- `help`

### clear
- Layer: Engine console processor
- Description: Clears console output scrollback immediately.
- Syntax: `clear`
- Example:
- `clear`

### echo
- Layer: Engine console processor
- Description: Prints text directly into console output.
- Syntax: `echo <text...>`
- Examples:
- `echo hi`
- `echo "worker spawned"`

## Queueable Commands (Executed via Routing in 32.3)

### reset_scene
- Layer: Engine / scene machine
- Description: Resets the active scene immediately.
- Syntax: `reset_scene`
- Example:
- `reset_scene`
- Result examples:
- `ok: scene reset`

### switch_scene
- Layer: Engine / scene machine
- Description: Switches active scene to known scene ID.
- Syntax: `switch_scene <scene_id>`
- Valid IDs: `a`, `b` (case-insensitive)
- Example:
- `switch_scene a`
- Result examples:
- `ok: switched to scene a`
- `ok: scene a already active`

### quit
- Layer: Engine / scene machine
- Description: Requests clean app exit.
- Syntax: `quit`
- Example:
- `quit`
- Result examples:
- `ok: quit requested`

### despawn
- Layer: Active scene debug hook
- Description: Queues a despawn intent for an internal numeric entity ID; apply happens at tick safe point.
- Syntax: `despawn <entity_id>`
- Example:
- `despawn 42`
- Result examples:
- `ok: queued despawn entity 42`
- `error: entity 42 not found`
- `error: active scene does not support this command`

### spawn
- Layer: Active scene debug hook
- Description: Queues a spawn intent by `def_name`, with optional world coordinates; apply happens at tick safe point.
- Syntax: `spawn <def_name> [x y]`
- Examples:
- `spawn proto.worker`
- `spawn proto.worker 1.5 -2.0`
- Defaults:
- If `[x y]` is omitted, spawn position priority is: cursor world position (if available), else player position, else origin.
- Special case:
- `spawn proto.player` creates an AI-controlled actor and does not replace the authoritative `player_id`.
- Result examples:
- `ok: queued spawn 'proto.worker' at (1.50, -2.00)`
- `error: unknown entity def 'proto.unknown'`
- `error: active scene does not support this command`

### select
- Layer: Active scene debug hook
- Description: Selects a selectable entity by runtime entity ID.
- Syntax: `select <entity_id>`
- Example:
- `select 42`
- Result examples:
- `ok: selected entity 42`
- `error: entity 42 not found`
- `error: entity 42 is not selectable`

### order.move
- Layer: Active scene debug hook
- Description: Queues a move order for the currently selected orderable actor.
- Syntax: `order.move <x> <y>`
- Example:
- `order.move 3.5 -1.25`
- Targeting notes:
- Selected `Settler`: snaps goal tile with `goal_tile = world_to_tile(<x,y>)`, then moves to that tile center.
- Selected `Settler`: creates/assigns a `MoveToPoint` job (first-class job path), then runs movement via job phases.
- Selected `PlayerPawn`: keeps direct world-point movement.
- Result examples:
- `ok: queued move for entity 42 to (3.50, -1.25)`
- `error: no selected entity`
- `error: selected entity 42 is not an orderable pawn`
- `error: selected entity 42 is not an actor`

### order.interact
- Layer: Active scene debug hook
- Description: Queues an interaction order from selected actor to a target entity ID.
- Syntax: `order.interact <target_entity_id>`
- Example:
- `order.interact 77`
- Targeting notes:
- Selected `Settler`: creates/assigns a `UseInteractable` job and interaction starts through the existing interaction pipeline when in range.
- Selected `PlayerPawn`: keeps direct interaction enqueue behavior.
- Result examples:
- `ok: queued interact actor 42 target 77`
- `error: no selected entity`
- `error: selected entity 42 is not an orderable pawn`
- `error: target entity 77 not found`
- `error: target entity 77 is not interactable`

### pause_sim
- Layer: Engine loop simulation control
- Description: Pauses automatic frame-driven simulation stepping while rendering continues.
- Syntax: `pause_sim`
- Example:
- `pause_sim`
- Result examples:
- `ok: sim paused`

### resume_sim
- Layer: Engine loop simulation control
- Description: Resumes automatic frame-driven simulation stepping.
- Syntax: `resume_sim`
- Example:
- `resume_sim`
- Result examples:
- `ok: sim resumed`

### tick
- Layer: Engine loop simulation control
- Description: Queues exact fixed simulation ticks while paused (or while running).
- Syntax: `tick <steps>`
- Example:
- `tick 60`
- Result examples:
- `ok: queued tick 60`

### thruport.status
- Layer: Engine loop runtime hooks / game thruport pump
- Description: Prints current thruport transport status snapshot for automation.
- Syntax: `thruport.status`
- Example:
- `thruport.status`
- Result example:
- `thruport.status v1 enabled:1 telemetry:1 clients:1`

### thruport.telemetry
- Layer: Engine loop runtime hooks / game thruport pump
- Description: Toggles remote telemetry frame streaming for the current process session.
- Syntax: `thruport.telemetry <on|off>`
- Examples:
- `thruport.telemetry on`
- `thruport.telemetry off`
- Result examples:
- `ok: thruport.telemetry v1 enabled:1`
- `ok: thruport.telemetry v1 enabled:0`

### dump.state
- Layer: Engine queueable -> active scene debug hook
- Description: Prints deterministic state probe line for automation assertions.
- Syntax: `dump.state`
- Example:
- `dump.state`
- Result example:
- `ok: dump.state v1 | player:1@(0.00,0.00) | cam:(0.00,0.00,1.00) | sel:1 | tgt:none | cnt:ent:3 act:2 int:1 | ev:1 | evk:is:0 ic:0 dm:0 dd:0 sa:1 se:0 | in:2 | ink:sp:0 mt:1 de:0 dmg:0 add:0 rem:0 si:1 ci:0 ca:0 | in_bad:0`

### dump.ai
- Layer: Engine queueable -> active scene debug hook
- Description: Prints deterministic AI probe line with state counts and nearest-agent preview.
- Syntax: `dump.ai`
- Example:
- `dump.ai`
- Result example:
- `ok: dump.ai v1 | cnt:id:0 wa:1 ch:2 use:0 | near:4@1.00,7@2.50`

### scenario.setup
- Layer: Engine queueable -> active scene debug hook (scene-owned implementation)
- Description: Sets up a deterministic scenario layout for automation preconditions.
- Syntax: `scenario.setup <scenario_id>`
- Supported scenario IDs (GameplayScene): `combat_chaser`, `visual_sandbox`, `nav_sandbox`
- Example:
- `scenario.setup combat_chaser`
- `scenario.setup visual_sandbox`
- `scenario.setup nav_sandbox`
- Result examples:
- `ok: scenario.setup combat_chaser player:1 chaser:2 dummy:3`
- `ok: scenario.setup visual_sandbox player:1 prop:2 wall:3 floor:4`
- `ok: scenario.setup nav_sandbox player:1 settler:2`
- `error: unknown scenario 'foo'`
- `error: active scene does not support this command`

### floor.set
- Layer: Engine queueable -> active scene debug hook (scene-owned implementation)
- Description: Sets the gameplay active floor filter used for rendering, picking, and interaction targeting.
- Syntax: `floor.set <rooftop|main|basement>`
- Examples:
- `floor.set rooftop`
- `floor.set main`
- `floor.set basement`
- Result examples:
- `ok: floor.set v1 active:basement`
- `error: invalid floor 'attic' (expected rooftop|main|basement)`
- `error: active scene does not support this command`

### input.key_down
- Layer: Engine loop input bridge
- Description: Enqueues a synthetic key-down event for next tick input snapshot merge.
- Syntax: `input.key_down <key>`
- Supported keys: `w`, `a`, `s`, `d`, `up`, `down`, `left`, `right`, `i`, `j`, `k`, `l`
- Example:
- `input.key_down w`
- Result examples:
- `ok: injected input.key_down w`

### input.key_up
- Layer: Engine loop input bridge
- Description: Enqueues a synthetic key-up event for next tick input snapshot merge.
- Syntax: `input.key_up <key>`
- Supported keys: `w`, `a`, `s`, `d`, `up`, `down`, `left`, `right`, `i`, `j`, `k`, `l`
- Example:
- `input.key_up w`
- Result examples:
- `ok: injected input.key_up w`

### input.mouse_move
- Layer: Engine loop input bridge
- Description: Enqueues a synthetic cursor move in window pixel space.
- Syntax: `input.mouse_move <x> <y>`
- Example:
- `input.mouse_move 640 360`
- Result examples:
- `ok: injected input.mouse_move 640 360`

### input.mouse_down
- Layer: Engine loop input bridge
- Description: Enqueues a synthetic mouse-button down edge.
- Syntax: `input.mouse_down <button>`
- Supported buttons: `left`, `right`
- Example:
- `input.mouse_down right`
- Result examples:
- `ok: injected input.mouse_down right`

### input.mouse_up
- Layer: Engine loop input bridge
- Description: Enqueues a synthetic mouse-button up event.
- Syntax: `input.mouse_up <button>`
- Supported buttons: `left`, `right`
- Example:
- `input.mouse_up right`
- Result examples:
- `ok: injected input.mouse_up right`

## Notes and Limitations

- Queueable commands are still parsed in tools, then routed for execution in the loop.
- `spawn`/`despawn` enqueue gameplay intents and world mutation occurs once per tick at the gameplay safe apply point.
- `spawn` uses the current gameplay active floor; switch floors first with `floor.set` if needed.
- `select` updates scene selection immediately; `order.move` and `order.interact` enqueue gameplay intents and apply at gameplay safe points.
- `floor.set` immediately changes active floor visibility/interaction filters via the scene debug-command seam.
- `pause_sim` affects only simulation stepping; rendering/frame pacing continues normally.
- `tick <steps>` advances the same fixed update path used by normal gameplay; no alternate loop exists.
- `thruport.status` prints exactly one status line with schema `thruport.status v1 enabled:<0|1> telemetry:<0|1> clients:<u32>`.
- `thruport.status telemetry:<0|1>` reflects the current runtime toggle value (startup default comes from `PROTOGE_THRUPORT_TELEMETRY`).
- `dump.state` / `dump.ai` are versioned text probes intended for remote automation checks without reading pixels.
- `input.*` commands enqueue synthetic input events that are applied once at `InputCollector::snapshot_for_tick` and merged into the normal input snapshot.
- If the authoritative player is missing, gameplay auto-spawns exactly one authoritative player actor on tick apply.
- `DebugCommand` stays in tools/engine layer; only `spawn`/`despawn` map one-way into scene-facing `SceneDebugCommand`.
- Selection/order automation commands are deterministic and avoid pixel/camera dependency:
- `select <entity_id>`
- `order.move <x> <y>`
- `order.interact <target_entity_id>`
- `order.move` / `order.interact` target the currently selected orderable pawn (`PlayerPawn` or `Settler`); NPC actors remain selectable for inspection/debug context but are non-orderable.
- Settler `order.move` final stop is deterministic tile-center snap (`world_to_tile` of command target), not arbitrary world-point exact stop.
- Reassigning a Settler job is deterministic and immediate: existing assigned job is failed in the same tick, locomotion/nav state is cleared, and any active interaction is canceled before the new job starts.
- Processor prints parse errors with usage hints.
- Unknown commands print `error: unknown command '<name>'. try: help`.

## Remote Wire Channels

- TCP remote lines are channel-tagged at send time:
- `C ` prefix: control lines (`ok:`, `error:`, `dump.*`, `sync`, `thruport.status`, `thruport.ready`).
- `T ` prefix: telemetry frame lines (`thruport.frame ...`).
- Exactly one space follows the channel tag.
- Local in-window console text is unchanged (no `C `/`T ` prefixes).

Example remote transcript snippet:
- `C thruport.ready v1 port:46001`
- `C ok: sim paused`
- `C ok: queued tick 1`
- `T thruport.frame v1 tick:42 paused:1 qtick:0 ev:1 in:1 in_bad:0`
- `C ok: thruport.telemetry v1 enabled:0`

## Update Rule

When any command is added, removed, renamed, or behavior changes:
- Update this file in the same change.
- Include command name, syntax, examples, defaults, and layer ownership (`Engine` or `Game/Scene`).
