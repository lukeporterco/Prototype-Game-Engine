use std::env;
use std::io;
use std::process::ExitCode;

use thruport_cli::{run, CommandKind, CommonOptions};

fn main() -> ExitCode {
    match run_cli() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run_cli() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Err(usage_text());
    }
    if args[0] == "-h" || args[0] == "--help" {
        print_usage();
        return Ok(());
    }

    let mut options = CommonOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --port".to_string())?;
                options.port = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --port value '{value}' (expected u16)"))?;
                index += 2;
            }
            "--timeout-ms" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --timeout-ms".to_string())?;
                options.timeout_ms = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-ms value '{value}' (expected u64)"))?;
                index += 2;
            }
            "--retry-ms" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --retry-ms".to_string())?;
                options.retry_ms = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --retry-ms value '{value}' (expected u64)"))?;
                index += 2;
            }
            "--include-telemetry" => {
                options.include_telemetry = true;
                index += 1;
            }
            _ => break,
        }
    }

    let command = args
        .get(index)
        .ok_or_else(|| "missing subcommand".to_string())?
        .as_str();
    let command_args = &args[(index + 1)..];

    let kind = match command {
        "wait-ready" => {
            if !command_args.is_empty() {
                return Err("wait-ready takes no arguments".to_string());
            }
            CommandKind::WaitReady
        }
        "send" => {
            if command_args.is_empty() {
                return Err("send requires a command payload".to_string());
            }
            CommandKind::Send {
                command: command_args.join(" "),
            }
        }
        "script" => {
            if command_args.is_empty() {
                return Err("script requires a file path".to_string());
            }
            let path = command_args[0].clone();
            let mut barrier = false;
            for arg in &command_args[1..] {
                if arg == "--barrier" {
                    barrier = true;
                } else {
                    return Err(format!(
                        "unknown script argument '{arg}' (expected --barrier)"
                    ));
                }
            }
            CommandKind::Script { path, barrier }
        }
        "barrier" => {
            if !command_args.is_empty() {
                return Err("barrier takes no arguments".to_string());
            }
            CommandKind::Barrier
        }
        other => return Err(format!("unknown subcommand '{other}'")),
    };

    run(kind, options, &mut io::stdout())
}

fn print_usage() {
    println!("{}", usage_text());
}

fn usage_text() -> String {
    [
        "thruport_cli - deterministic thruport client",
        "",
        "Usage:",
        "  thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] wait-ready",
        "  thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] send <command...>",
        "  thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] script <file> [--barrier]",
        "  thruport_cli [--port <u16>] [--timeout-ms <u64>] [--retry-ms <u64>] [--include-telemetry] barrier",
        "",
        "Defaults:",
        "  --port 46001",
        "  --timeout-ms 5000",
        "  --retry-ms 100",
    ]
    .join("\n")
}
