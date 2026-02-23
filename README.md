# Prototype Game Engine

Prototype-first Rust game engine + colony-sim vertical slice scaffold.

## Primary docs
- Infrastructure reference: `docs/PROTOGE_INFRASTRUCTURE_REFERENCE.md`
- Content pipeline contract: `docs/content_pipeline_contract_v1.md`
- Thruport CLI harness usage: `docs/thruport_cli.md`
- Test discovery helper workflow: `docs/test_helper.md`
- Working context log: `CODEXNOTES.md`

## Run

```powershell
cargo run
```

## Current baseline (Tickets 0-24)
- Fixed-step loop with decoupled render cadence.
- Persistent dual-scene runtime with deterministic switching/reset semantics.
- Entity rendering via placeholder or sprites.
- Tilemap ground layer + grid pass + interaction affordance pass.
- Camera pan + discrete zoom with clamps and inverse-safe transforms.
- Save/load v3 with stable `save_id` references.
- XML authoring compiled to binary content packs; runtime uses compiled data only.
- Lifetime-safe window ownership (`Arc<Window>`) and lifetime-free renderer.
- FPS cap knob in `LoopConfig.fps_cap`, uncapped shown as `U+221E` (infinity).

## Controls
- Move: `W/A/S/D` or arrow keys
- Camera pan: `I/J/K/L`
- Zoom: mouse wheel, `=`, `-`, numpad `+/-`
- Scene switch: `Tab` (edge-triggered)
- Overlay toggle: `F3` (edge-triggered)
- Save: `F5` (edge-triggered)
- Load: `F9` (edge-triggered)
- Quit: `Esc` or window close

## Environment variables
- `PROTOGE_ROOT`: explicit project root override
- `PROTOGE_ENABLED_MODS`: ordered comma-separated enabled mod list
- `PROTOGE_SLOW_FRAME_MS`: artificial per-frame debug delay

## Validation commands

```powershell
cargo fmt --all -- --check
cargo test -p engine
cargo test -p game
```
