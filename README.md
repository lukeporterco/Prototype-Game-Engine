# Prototype Game Engine

Prototype-first Rust engine bootstrap for Proto GE.

## Run

```powershell
cargo run
```

This opens a window, runs the main loop, logs periodic loop metrics, and exits cleanly when you close the window or press `Esc`.

## Root Resolution

At startup the app resolves `root`, `assets/base`, `mods`, and `cache` using this order:

1. `PROTOGE_ROOT` environment variable (if set)
2. Otherwise, walk upward from the executable directory and pick the first directory that contains:
   - `Cargo.toml`
   - and either `crates/` or `assets/`

If no matching root is found, startup fails fast with instructions.

## Loop and Metrics

- Fixed timestep simulation runs at 60 TPS by default.
- Rendering runs separately from simulation updates.
- Structured loop metrics are logged once per second:
  - `fps`
  - `tps`
  - `frame_time_ms`
- Simulation backlog is clamped to prevent runaway spirals on slow frames.
- Quit paths:
  - Window close button
  - `Esc` key

## Input and Movement (Ticket 3)

- Engine maps keyboard to actions: `MoveUp/Down/Left/Right`, `Quit`.
- Supported movement keys: `WASD` and arrow keys.
- Game scene consumes engine `InputSnapshot` actions with no `winit` dependency.
- Player movement runs in fixed-timestep update (`5.0` units/second), so distance is stable across FPS changes.
- Game exposes optional debug title text (`Scene::debug_title`), and engine applies it to the window title.

## Scenes and Entities

- Two hardcoded in-memory scenes are active.
- Press `Tab` to switch between Scene A and Scene B at runtime.
- Scene switching performs lifecycle in order: `unload -> clear -> load`.
- Entity model is engine-owned:
  - `EntityId` (session-unique)
  - `Transform` (`position` + optional `rotation_radians`)
  - renderer-agnostic `RenderableDesc`
- Game rules read an engine `InputSnapshot` in `Scene.update(...)`, so game code has no `winit` dependency.

## Slow Frame Simulation (Manual Test)

Use this to force an artificial per-frame delay and verify sim clamping behavior:

PowerShell:

```powershell
$env:PROTOGE_SLOW_FRAME_MS="250"
cargo run
Remove-Item Env:PROTOGE_SLOW_FRAME_MS
```

Bash/zsh:

```bash
export PROTOGE_SLOW_FRAME_MS="250"
cargo run
unset PROTOGE_SLOW_FRAME_MS
```

## Optional Override

PowerShell:

```powershell
$env:PROTOGE_ROOT="C:\path\to\Prototype Game Engine"
cargo run
```

Bash/zsh:

```bash
export PROTOGE_ROOT="/path/to/Prototype Game Engine"
cargo run
```

## Troubleshooting

If you see a root-detection error, set `PROTOGE_ROOT` to the repo root and rerun.
