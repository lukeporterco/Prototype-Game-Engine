# Purpose
This is the canonical quick-start for opening a fresh game process with thruport enabled, waiting for readiness, and running deterministic `thruport_cli` command sessions.

# What thruport is
Thruport is the TCP remote console path for this project. The game process listens on localhost when enabled, emits control (`C `) and telemetry (`T `) lines, and supports deterministic automation through console commands like `sync`, `scenario.setup`, `dump.state`, and `thruport.telemetry`.

# Activation (env vars and what each does)
- `PROTOGE_THRUPORT`
  - Enables thruport listener when set to `1`.
  - Any other value (or unset) means disabled.
- `PROTOGE_THRUPORT_PORT`
  - Listener port as `u16`.
  - If unset or invalid, default is `46001`.
- `PROTOGE_THRUPORT_TELEMETRY`
  - Initial telemetry streaming toggle at startup when set to `1`.
  - Any other value (or unset) means telemetry starts off.

Authoritative code anchors:
- `crates/game/src/app/dev_thruport.rs` (`PROTOGE_THRUPORT`, `PROTOGE_THRUPORT_PORT`, default `46001`, ready line)
- `crates/engine/src/app/loop_runner.rs` (`PROTOGE_THRUPORT_TELEMETRY`, `ok: sync`, `ok: thruport.telemetry ...`)

# Start the game with thruport enabled (PowerShell + bash/zsh examples)
PowerShell (terminal 1):
```powershell
$env:PROTOGE_THRUPORT='1'
$env:PROTOGE_THRUPORT_PORT='46001'
$env:PROTOGE_THRUPORT_TELEMETRY='1'
cargo run -p game
```

bash/zsh (terminal 1):
```bash
export PROTOGE_THRUPORT=1
export PROTOGE_THRUPORT_PORT=46001
export PROTOGE_THRUPORT_TELEMETRY=1
cargo run -p game
```

# Connect and verify readiness (use `thruport_cli wait-ready`)
Always gate on readiness before sending commands.

PowerShell (terminal 2):
```powershell
thruport_cli --port 46001 wait-ready
```

bash/zsh (terminal 2):
```bash
thruport_cli --port 46001 wait-ready
```

Expected success payload:
- `thruport.ready v1 port:46001`

# Deterministic command execution model (explain `sync` barrier and that CLI uses deterministic completion, not quiet windows)
- `thruport_cli send <command...>` uses deterministic completion by issuing an internal `sync` barrier and waiting for control payload `ok: sync`.
- The internal barrier ack is not printed by default.
- `thruport_cli barrier` is the explicit barrier path when you need a visible sync boundary between steps.
- `thruport_cli script <file> --barrier` keeps script behavior deterministic with one explicit final barrier.
- Quiet-window behavior (`--quiet-ms`) is fallback only if `sync` is unavailable.

# Common workflows (examples)
Smoke sequence (deterministic):

PowerShell:
```powershell
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 send pause_sim
thruport_cli --port 46001 send reset_scene
thruport_cli --port 46001 send scenario.setup combat_chaser
thruport_cli --port 46001 send dump.state
thruport_cli --port 46001 send dump.ai
thruport_cli --port 46001 barrier
```

bash/zsh:
```bash
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 send pause_sim
thruport_cli --port 46001 send reset_scene
thruport_cli --port 46001 send scenario.setup combat_chaser
thruport_cli --port 46001 send dump.state
thruport_cli --port 46001 send dump.ai
thruport_cli --port 46001 barrier
```

Telemetry gating (off, tick, barrier, on, tick, barrier):

PowerShell:
```powershell
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 send thruport.telemetry off
thruport_cli --port 46001 --include-telemetry send tick 3
thruport_cli --port 46001 barrier
thruport_cli --port 46001 send thruport.telemetry on
thruport_cli --port 46001 --include-telemetry send tick 3
thruport_cli --port 46001 barrier
```

bash/zsh:
```bash
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 send "thruport.telemetry off"
thruport_cli --port 46001 --include-telemetry send "tick 3"
thruport_cli --port 46001 barrier
thruport_cli --port 46001 send "thruport.telemetry on"
thruport_cli --port 46001 --include-telemetry send "tick 3"
thruport_cli --port 46001 barrier
```

Multi-client pattern (one telemetry reader, one control sender):

PowerShell:
```powershell
# terminal A (telemetry reader)
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 --include-telemetry send thruport.telemetry on
thruport_cli --port 46001 --include-telemetry send tick 100
thruport_cli --port 46001 barrier

# terminal B (control sender)
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 send thruport.status
thruport_cli --port 46001 send dump.ai
thruport_cli --port 46001 barrier
```

bash/zsh:
```bash
# terminal A (telemetry reader)
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 --include-telemetry send "thruport.telemetry on"
thruport_cli --port 46001 --include-telemetry send "tick 100"
thruport_cli --port 46001 barrier

# terminal B (control sender)
thruport_cli --port 46001 wait-ready
thruport_cli --port 46001 send thruport.status
thruport_cli --port 46001 send dump.ai
thruport_cli --port 46001 barrier
```

# Troubleshooting and retries
- Connection refused after launch:
  - Keep the game running and retry readiness.
  - Command: `thruport_cli --port 46001 --timeout-ms 10000 wait-ready`
- Host aborts long-lived sockets:
  - Prefer reconnect-per-invocation (`thruport_cli send ...`) and short script runs.
  - If a section fails due to disconnect, restart game and re-run only that section.
- Telemetry mixing in outputs:
  - Default CLI output is control-only.
  - Enable telemetry only when needed via `--include-telemetry` and/or `thruport.telemetry on`.
  - Use barriers between phases to keep transcript boundaries clear.

# Appendix: command reference pointers
- Thruport CLI contract and options: `docs/thruport_cli.md`
- Console command syntax and schemas: `CONSOLE_COMMANDS.md`
- If anything appears out of date, discover via grep:
  - `rg -n "PROTOGE_THRUPORT|PROTOGE_THRUPORT_PORT|PROTOGE_THRUPORT_TELEMETRY|thruport.ready|thruport.telemetry|wait-ready|ok: sync|\\bsync\\b" -S .`
