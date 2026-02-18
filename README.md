# Prototype Game Engine

Minimal Rust bootstrap for Proto GE (Ticket 0).

## Run

```powershell
cargo run
```

This should print a startup banner, resolved content/cache paths, and exit cleanly.

## Root Resolution

At startup the app resolves `root`, `assets/base`, `mods`, and `cache` using this order:

1. `PROTOGE_ROOT` environment variable (if set)
2. Otherwise, walk upward from the executable directory and pick the first directory that contains:
   - `Cargo.toml`
   - and either `crates/` or `assets/`

If no matching root is found, startup fails fast with instructions.

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
