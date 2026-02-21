# Console Commands (Developer Reference)

Status: `Ticket 32.3` implements routing/execution for queueable commands.

## Console Controls

- Toggle console: `` ` `` (Backquote)
- While open:
- Type text into prompt
- `Backspace` deletes one character
- `Enter` submits current line
- `Up` / `Down` browse submission history
- `Escape` closes console and clears current input line

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
- Result examples:
- `ok: queued spawn 'proto.worker' at (1.50, -2.00)`
- `error: unknown entity def 'proto.unknown'`
- `error: active scene does not support this command`

## Notes and Limitations

- Queueable commands are still parsed in tools, then routed for execution in the loop.
- `spawn`/`despawn` enqueue gameplay intents and world mutation occurs once per tick at the gameplay safe apply point.
- `DebugCommand` stays in tools/engine layer; only `spawn`/`despawn` map one-way into scene-facing `SceneDebugCommand`.
- Processor prints parse errors with usage hints.
- Unknown commands print `error: unknown command '<name>'. try: help`.

## Update Rule

When any command is added, removed, renamed, or behavior changes:
- Update this file in the same change.
- Include command name, syntax, examples, defaults, and layer ownership (`Engine` or `Game/Scene`).
