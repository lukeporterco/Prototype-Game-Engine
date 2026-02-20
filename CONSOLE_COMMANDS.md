# Console Commands (Developer Reference)

Status: `Ticket 32.2` implemented parsing, validation, help, and queueing.

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
- Queueable actions (parsed and enqueued as `DebugCommand`, not executed yet):
- `reset_scene`
- `switch_scene`
- `quit`
- `despawn`
- `spawn`

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

## Queueable Commands (Parse-Only in 32.2)

### reset_scene
- Layer: Engine command queue
- Description: Queues a scene reset command.
- Syntax: `reset_scene`
- Example:
- `reset_scene`

### switch_scene
- Layer: Engine command queue
- Description: Queues scene switch to known scene ID.
- Syntax: `switch_scene <scene_id>`
- Valid IDs: `a`, `b` (case-insensitive)
- Example:
- `switch_scene a`

### quit
- Layer: Engine command queue
- Description: Queues app quit request.
- Syntax: `quit`
- Example:
- `quit`

### despawn
- Layer: Engine command queue
- Description: Queues despawn by internal numeric entity ID.
- Syntax: `despawn <entity_id>`
- Example:
- `despawn 42`

### spawn
- Layer: Engine command queue
- Description: Queues spawn by `def_name`, with optional world coordinates.
- Syntax: `spawn <def_name> [x y]`
- Examples:
- `spawn proto.worker`
- `spawn proto.worker 1.5 -2.0`
- Defaults:
- If `[x y]` is omitted, command queues without explicit position.

## Notes and Limitations

- Queueable commands are validated and queued only in Ticket 32.2.
- No queueable command mutates live scene/world yet.
- Processor prints parse errors with usage hints.
- Unknown commands print `error: unknown command '<name>'. try: help`.

## Update Rule

When any command is added, removed, renamed, or behavior changes:
- Update this file in the same change.
- Include command name, syntax, examples, defaults, and layer ownership (`Engine` or `Game/Scene`).
