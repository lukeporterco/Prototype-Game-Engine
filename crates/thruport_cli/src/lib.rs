use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

pub const DEFAULT_PORT: u16 = 46001;
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
pub const DEFAULT_RETRY_MS: u64 = 100;
pub const DEFAULT_QUIET_MS: u64 = 250;
const MAX_RETRY_BACKOFF_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineChannel {
    Control,
    Telemetry,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    pub channel: LineChannel,
    pub payload: String,
}

#[derive(Debug, Clone, Copy)]
pub struct CommonOptions {
    pub port: u16,
    pub timeout_ms: u64,
    pub retry_ms: u64,
    pub include_telemetry: bool,
}

impl Default for CommonOptions {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            retry_ms: DEFAULT_RETRY_MS,
            include_telemetry: false,
        }
    }
}

pub enum CommandKind {
    WaitReady,
    Send { command: String },
    Script { path: String, barrier: bool },
    Barrier,
}

struct Session {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
}

pub fn parse_wire_line(raw: &str) -> ParsedLine {
    let trimmed = raw.trim_end_matches(['\r', '\n']);
    if let Some(payload) = trimmed.strip_prefix("C ") {
        return ParsedLine {
            channel: LineChannel::Control,
            payload: payload.to_string(),
        };
    }
    if let Some(payload) = trimmed.strip_prefix("T ") {
        return ParsedLine {
            channel: LineChannel::Telemetry,
            payload: payload.to_string(),
        };
    }
    ParsedLine {
        channel: LineChannel::Unknown,
        payload: trimmed.to_string(),
    }
}

pub fn should_print_line(line: &ParsedLine, include_telemetry: bool) -> bool {
    match line.channel {
        LineChannel::Control => true,
        LineChannel::Telemetry => include_telemetry,
        LineChannel::Unknown => false,
    }
}

pub fn is_ready_payload(payload: &str) -> bool {
    payload.starts_with("thruport.ready v1 port:")
}

pub fn is_sync_ok_payload(payload: &str) -> bool {
    payload == "ok: sync"
}

pub fn parse_script_commands(content: &str) -> Vec<String> {
    let mut commands = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        commands.push(trimmed.to_string());
    }
    commands
}

pub fn run<W: Write>(kind: CommandKind, opts: CommonOptions, stdout: &mut W) -> Result<(), String> {
    let timeout = Duration::from_millis(opts.timeout_ms);
    let retry_base = Duration::from_millis(opts.retry_ms.max(1));
    let mut session = connect_and_wait_ready(opts.port, timeout, retry_base, |line| {
        emit_line(stdout, line, opts.include_telemetry)
    })?;

    match kind {
        CommandKind::WaitReady => Ok(()),
        CommandKind::Send { command } => {
            send_line(&mut session.writer, &command)?;
            read_until_quiet(
                &mut session.reader,
                timeout,
                Duration::from_millis(DEFAULT_QUIET_MS),
                |line| emit_line(stdout, line, opts.include_telemetry),
            )
        }
        CommandKind::Script { path, barrier } => {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read script file '{path}': {error}"))?;
            let commands = parse_script_commands(&content);
            for command in commands {
                send_line(&mut session.writer, &command)?;
                read_until_quiet(
                    &mut session.reader,
                    timeout,
                    Duration::from_millis(DEFAULT_QUIET_MS),
                    |line| emit_line(stdout, line, opts.include_telemetry),
                )?;
            }
            if barrier {
                send_barrier_and_wait_ack(&mut session, timeout, |line| {
                    emit_line(stdout, line, opts.include_telemetry)
                })?;
            }
            Ok(())
        }
        CommandKind::Barrier => send_barrier_and_wait_ack(&mut session, timeout, |line| {
            emit_line(stdout, line, opts.include_telemetry)
        }),
    }
}

fn connect_and_wait_ready<F>(
    port: u16,
    timeout: Duration,
    retry_base: Duration,
    mut on_line: F,
) -> Result<Session, String>
where
    F: FnMut(&ParsedLine),
{
    let deadline = Instant::now() + timeout;
    let mut attempt = 0u32;

    while Instant::now() < deadline {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(writer) => {
                writer
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .map_err(|error| format!("failed to set socket read timeout: {error}"))?;
                let reader_stream = writer
                    .try_clone()
                    .map_err(|error| format!("failed to clone socket stream: {error}"))?;
                let mut session = Session {
                    writer,
                    reader: BufReader::new(reader_stream),
                };

                match wait_for_ready(&mut session.reader, deadline, &mut on_line) {
                    WaitReadyOutcome::Ready => return Ok(session),
                    WaitReadyOutcome::Timeout => break,
                    WaitReadyOutcome::Disconnected => {
                        if Instant::now() >= deadline {
                            break;
                        }
                    }
                    WaitReadyOutcome::IoError(error) => {
                        return Err(format!(
                            "socket read error while waiting for ready: {error}"
                        ));
                    }
                }
            }
            Err(_) => {
                if Instant::now() >= deadline {
                    break;
                }
            }
        }

        let shift = attempt.min(8);
        let backoff_ms = (retry_base.as_millis() as u64)
            .saturating_mul(1u64 << shift)
            .min(MAX_RETRY_BACKOFF_MS);
        let sleep_for = Duration::from_millis(backoff_ms.max(1));
        let now = Instant::now();
        if now + sleep_for >= deadline {
            break;
        }
        thread::sleep(sleep_for);
        attempt = attempt.saturating_add(1);
    }

    Err(format!(
        "timed out waiting for thruport ready on 127.0.0.1:{port}"
    ))
}

enum WaitReadyOutcome {
    Ready,
    Timeout,
    Disconnected,
    IoError(io::Error),
}

fn wait_for_ready<F>(
    reader: &mut BufReader<TcpStream>,
    deadline: Instant,
    on_line: &mut F,
) -> WaitReadyOutcome
where
    F: FnMut(&ParsedLine),
{
    loop {
        match read_one_line(reader, deadline) {
            ReadOutcome::Line(raw) => {
                let parsed = parse_wire_line(&raw);
                on_line(&parsed);
                if parsed.channel == LineChannel::Control && is_ready_payload(&parsed.payload) {
                    return WaitReadyOutcome::Ready;
                }
            }
            ReadOutcome::NoData => {}
            ReadOutcome::Disconnected => return WaitReadyOutcome::Disconnected,
            ReadOutcome::DeadlineExceeded => return WaitReadyOutcome::Timeout,
            ReadOutcome::IoError(error) => return WaitReadyOutcome::IoError(error),
        }
    }
}

fn send_barrier_and_wait_ack<F>(
    session: &mut Session,
    timeout: Duration,
    mut on_line: F,
) -> Result<(), String>
where
    F: FnMut(&ParsedLine),
{
    send_line(&mut session.writer, "sync")?;
    let deadline = Instant::now() + timeout;
    loop {
        match read_one_line(&mut session.reader, deadline) {
            ReadOutcome::Line(raw) => {
                let parsed = parse_wire_line(&raw);
                on_line(&parsed);
                if parsed.channel == LineChannel::Control && is_sync_ok_payload(&parsed.payload) {
                    return Ok(());
                }
            }
            ReadOutcome::NoData => {}
            ReadOutcome::Disconnected => {
                return Err("socket disconnected while waiting for barrier ack".to_string())
            }
            ReadOutcome::DeadlineExceeded => {
                return Err("timed out waiting for barrier ack (ok: sync)".to_string())
            }
            ReadOutcome::IoError(error) => {
                return Err(format!(
                    "socket read error while waiting for barrier ack: {error}"
                ))
            }
        }
    }
}

fn read_until_quiet<F>(
    reader: &mut BufReader<TcpStream>,
    timeout: Duration,
    quiet_window: Duration,
    mut on_line: F,
) -> Result<(), String>
where
    F: FnMut(&ParsedLine),
{
    let deadline = Instant::now() + timeout;
    let mut last_line_at = Instant::now();
    let mut saw_any_line = false;

    loop {
        match read_one_line(reader, deadline) {
            ReadOutcome::Line(raw) => {
                let parsed = parse_wire_line(&raw);
                on_line(&parsed);
                saw_any_line = true;
                last_line_at = Instant::now();
            }
            ReadOutcome::NoData => {
                let now = Instant::now();
                if (saw_any_line && now.saturating_duration_since(last_line_at) >= quiet_window)
                    || (!saw_any_line && now + quiet_window >= deadline)
                {
                    return Ok(());
                }
            }
            ReadOutcome::Disconnected => {
                return Err("socket disconnected while waiting for command output".to_string())
            }
            ReadOutcome::DeadlineExceeded => return Ok(()),
            ReadOutcome::IoError(error) => {
                return Err(format!(
                    "socket read error while waiting for command output: {error}"
                ))
            }
        }
    }
}

fn emit_line<W: Write>(stdout: &mut W, line: &ParsedLine, include_telemetry: bool) {
    if should_print_line(line, include_telemetry) {
        let _ = writeln!(stdout, "{}", line.payload);
    }
}

fn send_line(writer: &mut TcpStream, line: &str) -> Result<(), String> {
    writer
        .write_all(line.as_bytes())
        .map_err(|error| format!("failed to send command: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("failed to terminate command line: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("failed to flush command line: {error}"))
}

enum ReadOutcome {
    Line(String),
    NoData,
    Disconnected,
    DeadlineExceeded,
    IoError(io::Error),
}

fn read_one_line(reader: &mut BufReader<TcpStream>, deadline: Instant) -> ReadOutcome {
    if Instant::now() >= deadline {
        return ReadOutcome::DeadlineExceeded;
    }

    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => ReadOutcome::Disconnected,
        Ok(_) => ReadOutcome::Line(line),
        Err(error)
            if error.kind() == io::ErrorKind::WouldBlock
                || error.kind() == io::ErrorKind::TimedOut =>
        {
            ReadOutcome::NoData
        }
        Err(error) => ReadOutcome::IoError(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wire_line_handles_channels_and_crlf() {
        assert_eq!(
            parse_wire_line("C ok: sync\r\n"),
            ParsedLine {
                channel: LineChannel::Control,
                payload: "ok: sync".to_string(),
            }
        );
        assert_eq!(
            parse_wire_line("T thruport.frame v1 tick:1\n"),
            ParsedLine {
                channel: LineChannel::Telemetry,
                payload: "thruport.frame v1 tick:1".to_string(),
            }
        );
        assert_eq!(
            parse_wire_line("unknown line\r\n"),
            ParsedLine {
                channel: LineChannel::Unknown,
                payload: "unknown line".to_string(),
            }
        );
    }

    #[test]
    fn output_filter_defaults_control_only_and_allows_telemetry() {
        let control = ParsedLine {
            channel: LineChannel::Control,
            payload: "ok: sync".to_string(),
        };
        let telemetry = ParsedLine {
            channel: LineChannel::Telemetry,
            payload: "thruport.frame v1 tick:1".to_string(),
        };
        let unknown = ParsedLine {
            channel: LineChannel::Unknown,
            payload: "raw".to_string(),
        };

        assert!(should_print_line(&control, false));
        assert!(!should_print_line(&telemetry, false));
        assert!(!should_print_line(&unknown, false));
        assert!(should_print_line(&telemetry, true));
    }

    #[test]
    fn ready_and_barrier_matchers_work() {
        assert!(is_ready_payload("thruport.ready v1 port:46001"));
        assert!(!is_ready_payload("ok: sync"));
        assert!(is_sync_ok_payload("ok: sync"));
        assert!(!is_sync_ok_payload("ok: sim paused"));
    }

    #[test]
    fn parse_script_commands_ignores_blank_and_comment_lines() {
        let content = r#"
            # comment
            pause_sim

            tick 1
            # another
            dump.state
        "#;
        assert_eq!(
            parse_script_commands(content),
            vec![
                "pause_sim".to_string(),
                "tick 1".to_string(),
                "dump.state".to_string()
            ]
        );
    }

    #[test]
    fn transcript_scan_for_ready_is_deterministic() {
        let transcript = vec![
            parse_wire_line("T thruport.frame v1 tick:1"),
            parse_wire_line("C ok: sim paused"),
            parse_wire_line("C thruport.ready v1 port:46001"),
        ];
        let saw_ready = transcript
            .iter()
            .any(|line| line.channel == LineChannel::Control && is_ready_payload(&line.payload));
        assert!(saw_ready);
    }

    #[test]
    fn transcript_scan_for_barrier_ack_requires_sync_ok() {
        let before_ack = vec![
            parse_wire_line("C ok: sim paused"),
            parse_wire_line("T thruport.frame v1 tick:5"),
        ];
        let after_ack = vec![
            parse_wire_line("C ok: sim paused"),
            parse_wire_line("C ok: sync"),
        ];

        let before = before_ack
            .iter()
            .any(|line| line.channel == LineChannel::Control && is_sync_ok_payload(&line.payload));
        let after = after_ack
            .iter()
            .any(|line| line.channel == LineChannel::Control && is_sync_ok_payload(&line.payload));
        assert!(!before);
        assert!(after);
    }
}
