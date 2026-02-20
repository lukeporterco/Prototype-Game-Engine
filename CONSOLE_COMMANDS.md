# Console Commands (Developer Reference)

Status: `Ticket 32.1` implemented the console UI shell only. Command execution is not implemented yet.

## Current Console Behavior

- Toggle console: `` ` `` (Backquote)
- While open:
- Type text into prompt.
- `Backspace` deletes one character.
- `Enter` submits line and does two things: echoes `> <line>` into scrollback and queues raw `<line>` into pending engine console lines.
- `Up` / `Down` navigates submitted history.
- `Escape` closes console and clears current input line.

## Implemented Commands

None yet.

Notes:
- There is currently no parser/dispatcher bound to submitted lines.
- Submitted lines are queued for a future ticket to consume.
- Treat all entered text as raw input only at this stage.

## Update Rule

When any command is added, removed, renamed, or behavior changes:
- Update this file in the same change.
- Include command name, syntax, examples, defaults, and layer ownership (`Engine` or `Game/Scene`).
