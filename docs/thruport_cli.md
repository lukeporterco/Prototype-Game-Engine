# Thruport CLI Harness

`thruport_cli` is a lightweight repo-owned client for deterministic thruport automation.

It avoids ad-hoc shell socket pitfalls by handling:
- connect retry/backoff
- ready-line gating (`thruport.ready v1`)
- command send with deterministic readback
- barrier synchronization (`ok: sync`)

## Build

```powershell
cargo build -p thruport_cli
```

## Usage

```powershell
thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] wait-ready
thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] send <command...>
thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] script <file> [--barrier]
thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] barrier
```

Defaults:
- `--port 46001`
- `--timeout-ms 5000`
- `--retry-ms 100`

Output behavior:
- Default prints control payload lines only.
- `--include-telemetry` prints both control and telemetry payloads.
- Channel tags are stripped before printing.

## Examples

Wait for the remote listener to be ready:

```powershell
thruport_cli wait-ready
```

Send one command:

```powershell
thruport_cli send thruport.status
```

Run a script file and end on sync barrier:

```powershell
thruport_cli script .\minset.txt --barrier
```

Send a barrier directly:

```powershell
thruport_cli barrier
```
