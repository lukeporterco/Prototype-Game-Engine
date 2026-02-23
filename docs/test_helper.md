# Test Helper (Dev Workflow)
Last updated: 2026-02-23. Covers: Tickets 51-52.

This repo ships a small PowerShell helper for deterministic test discovery and single-test execution:

- Script: `scripts/test-helper.ps1`
- Supported packages: `engine`, `game`, `thruport_cli`

If `pwsh` is unavailable, use `powershell -ExecutionPolicy Bypass -File <script.ps1>`.

## Why this exists

Cargo test filtering is easy to misuse during iteration:

- Cargo supports one positional test-name filter token in `cargo test`.
- Mixing multiple partial tokens often leads to false starts and unintended matches.

This helper avoids that by:

- Listing canonical test names from `cargo test -p <pkg> -- --list`.
- Resolving regex to exactly one full test name before running with `--exact`.

## Usage

List tests in a package:

```powershell
pwsh -File scripts/test-helper.ps1 -Mode list -Package engine
```

List tests with regex filter:

```powershell
pwsh -File scripts/test-helper.ps1 -Mode list -Package game -Pattern "scenario_setup"
```

Run exactly one resolved test (safe mode):

```powershell
pwsh -File scripts/test-helper.ps1 -Mode run-one -Package game -Pattern "scenario_setup_combat_chaser_is_idempotent"
```

## Behavior

- `list`:
  - Prints canonical test names, one per line.
  - `-Pattern` is optional regex.
- `run-one`:
  - Requires `-Pattern`.
  - Fails if regex matches zero or multiple tests.
  - Runs resolved name with:
    - `cargo test -p <pkg> <full_test_name> -- --exact`

## Demonstrated output snippet

Example from `run-one`:

```text
Matched 1 test: app::gameplay::tests::scenario_setup_combat_chaser_is_idempotent
Running exact: cargo test -p game app::gameplay::tests::scenario_setup_combat_chaser_is_idempotent -- --exact
```
