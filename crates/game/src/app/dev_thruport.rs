use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use engine::RemoteConsoleLinePump;
use tracing::{info, warn};

pub(crate) trait ConsoleInputQueueHook {
    fn drain_pending_lines(&mut self, out: &mut Vec<String>);
}

pub(crate) trait ConsoleOutputTeeHook {
    fn tee_output_line(&mut self, line: &str);
}

pub(crate) trait InputInjectionHook {
    fn inject_input(&mut self, input: InjectedInput);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InjectedInput {
    NoOp,
    KeyDown,
    KeyUp,
    MouseMove { x: f32, y: f32 },
}

const THRUPORT_ENV_VAR: &str = "PROTOGE_THRUPORT";
const THRUPORT_PORT_ENV_VAR: &str = "PROTOGE_THRUPORT_PORT";
const THRUPORT_DIAG_ENV_VAR: &str = "PROTOGE_THRUPORT_DIAG";
const THRUPORT_DEFAULT_PORT: u16 = 46001;
const MAX_PENDING_TELEMETRY_BYTES_PER_CLIENT: usize = 256 * 1024;
const MAX_PENDING_CONTROL_BYTES_PER_CLIENT: usize = 256 * 1024;
const REMOTE_CONTROL_PREFIX: &str = "C ";
const REMOTE_TELEMETRY_PREFIX: &str = "T ";

pub(crate) struct DevThruportHooks {
    _console_input: Option<Box<dyn ConsoleInputQueueHook + Send>>,
    _console_output_tee: Option<Box<dyn ConsoleOutputTeeHook + Send>>,
    _input_injection: Option<Box<dyn InputInjectionHook + Send>>,
}

struct NoOpConsoleInputHook;
struct NoOpConsoleOutputTeeHook;
struct NoOpInputInjectionHook;

impl ConsoleInputQueueHook for NoOpConsoleInputHook {
    fn drain_pending_lines(&mut self, _out: &mut Vec<String>) {}
}

impl ConsoleOutputTeeHook for NoOpConsoleOutputTeeHook {
    fn tee_output_line(&mut self, _line: &str) {}
}

impl InputInjectionHook for NoOpInputInjectionHook {
    fn inject_input(&mut self, _input: InjectedInput) {}
}

impl DevThruportHooks {
    pub(crate) fn no_op() -> Self {
        Self {
            _console_input: None,
            _console_output_tee: None,
            _input_injection: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DevThruportConfig {
    enabled: bool,
    port: u16,
}

impl DevThruportConfig {
    fn from_env() -> Self {
        let enabled = parse_enabled_flag(std::env::var(THRUPORT_ENV_VAR).ok().as_deref());
        let raw_port = std::env::var(THRUPORT_PORT_ENV_VAR).ok();
        let port = match raw_port.as_deref() {
            Some(value) => match value.parse::<u16>() {
                Ok(parsed_port) => parsed_port,
                Err(_) => {
                    warn!(
                        value,
                        fallback_port = THRUPORT_DEFAULT_PORT,
                        "thruport_invalid_port_using_default"
                    );
                    THRUPORT_DEFAULT_PORT
                }
            },
            None => THRUPORT_DEFAULT_PORT,
        };
        Self { enabled, port }
    }
}

#[derive(Debug)]
enum DevThruportMode {
    Disabled,
    Enabled(TcpRemoteConsoleTransport),
}

pub(crate) struct DevThruport {
    mode: DevThruportMode,
    _hooks: DevThruportHooks,
    disconnect_reset_requested: bool,
}

#[derive(Debug)]
struct TcpRemoteConsoleTransport {
    listener: TcpListener,
    bound_port: u16,
    clients: Vec<ClientConn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutboundClass {
    Control,
    Telemetry,
}

#[derive(Debug)]
struct OutboundChunk {
    class: OutboundClass,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct OutboundChunkState {
    chunk: OutboundChunk,
    written: usize,
}

#[derive(Debug)]
struct ClientConn {
    stream: TcpStream,
    read_buf: Vec<u8>,
    active_chunk: Option<OutboundChunkState>,
    queued_chunks: VecDeque<OutboundChunk>,
    queued_control_bytes: usize,
    queued_telemetry_bytes: usize,
}

impl TcpRemoteConsoleTransport {
    fn bind_localhost(port: u16) -> io::Result<Self> {
        let addr = localhost_bind_addr(port);
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        let bound_port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            bound_port,
            clients: Vec::new(),
        })
    }

    fn poll_lines(&mut self, out: &mut Vec<String>) -> bool {
        self.accept_pending_clients();
        let removed_during_read = self.poll_client_lines(out);
        let removed_during_flush = self.flush_all_client_outbound();
        removed_during_read || removed_during_flush
    }

    fn accept_pending_clients(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, _addr)) => {
                    if let Err(err) = stream.set_nonblocking(true) {
                        warn!(error = %err, "thruport_client_nonblocking_failed");
                        continue;
                    }
                    if let Err(err) = stream.set_nodelay(true) {
                        warn!(error = %err, "thruport_client_nodelay_failed");
                    }
                    let mut client = ClientConn {
                        stream,
                        read_buf: Vec::new(),
                        active_chunk: None,
                        queued_chunks: VecDeque::new(),
                        queued_control_bytes: 0,
                        queued_telemetry_bytes: 0,
                    };
                    enqueue_control_line(&mut client, &ready_line_text(self.bound_port));
                    self.clients.push(client);
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => {
                    warn!(error = %err, "thruport_accept_failed");
                    break;
                }
            }
        }
    }

    fn poll_client_lines(&mut self, out: &mut Vec<String>) -> bool {
        let mut removed_any = false;
        let mut index = 0usize;
        while index < self.clients.len() {
            let mut disconnected = false;
            {
                let client = &mut self.clients[index];
                let mut chunk = [0u8; 1024];
                loop {
                    match client.stream.read(&mut chunk) {
                        Ok(0) => {
                            disconnected = true;
                            break;
                        }
                        Ok(bytes_read) => {
                            client.read_buf.extend_from_slice(&chunk[..bytes_read]);
                            drain_complete_lines(&mut client.read_buf, out);
                        }
                        Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                        Err(err) => {
                            warn!(error = %err, "thruport_client_read_failed");
                            disconnected = true;
                            break;
                        }
                    }
                }
            }

            if disconnected {
                self.clients.swap_remove(index);
                removed_any = true;
            } else {
                index += 1;
            }
        }
        removed_any
    }

    fn send_output_lines(&mut self, lines: &[String]) -> bool {
        for client in &mut self.clients {
            for line in lines {
                enqueue_control_line_with_cap(client, line, MAX_PENDING_CONTROL_BYTES_PER_CLIENT);
            }
        }
        self.flush_all_client_outbound()
    }

    fn send_frame_line(&mut self, line: &str) -> bool {
        for client in &mut self.clients {
            enqueue_telemetry_line_with_cap(client, line, MAX_PENDING_TELEMETRY_BYTES_PER_CLIENT);
        }
        self.flush_all_client_outbound()
    }

    fn flush_all_client_outbound(&mut self) -> bool {
        let mut removed_any = false;
        let mut index = 0usize;
        while index < self.clients.len() {
            let flush_result = {
                let client = &mut self.clients[index];
                flush_pending_chunks(
                    &mut client.active_chunk,
                    &mut client.queued_chunks,
                    &mut client.queued_control_bytes,
                    &mut client.queued_telemetry_bytes,
                    |payload| client.stream.write(payload),
                )
            };
            if let Err(err) = flush_result {
                warn!(error = %err, "thruport_client_write_failed");
                self.clients.swap_remove(index);
                removed_any = true;
            } else {
                index += 1;
            }
        }
        removed_any
    }
}

pub(crate) fn initialize(hooks: DevThruportHooks) -> DevThruport {
    exercise_forward_contracts_noop();

    let config = DevThruportConfig::from_env();
    let mode = if config.enabled {
        match TcpRemoteConsoleTransport::bind_localhost(config.port) {
            Ok(transport) => {
                info!(
                    line = %ready_line_text(transport.bound_port),
                    "thruport_ready_bound"
                );
                DevThruportMode::Enabled(transport)
            }
            Err(err) => {
                warn!(error = %err, port = config.port, "thruport_bind_failed_disabled");
                DevThruportMode::Disabled
            }
        }
    } else {
        DevThruportMode::Disabled
    };

    DevThruport {
        mode,
        _hooks: hooks,
        disconnect_reset_requested: false,
    }
}

fn exercise_forward_contracts_noop() {
    let mut input_queue = NoOpConsoleInputHook;
    let mut drained = Vec::new();
    input_queue.drain_pending_lines(&mut drained);

    let mut output_tee = NoOpConsoleOutputTeeHook;
    output_tee.tee_output_line("");

    let mut input_injection = NoOpInputInjectionHook;
    input_injection.inject_input(InjectedInput::NoOp);
    input_injection.inject_input(InjectedInput::KeyDown);
    input_injection.inject_input(InjectedInput::KeyUp);
    input_injection.inject_input(InjectedInput::MouseMove { x: 0.0, y: 0.0 });
}

impl DevThruport {
    fn poll_remote_lines(&mut self, out: &mut Vec<String>) {
        if let DevThruportMode::Enabled(transport) = &mut self.mode {
            let removed_any = transport.poll_lines(out);
            if removed_any {
                self.disconnect_reset_requested = true;
            }
        }
    }

    fn enabled_flag(&self) -> u32 {
        match self.mode {
            DevThruportMode::Enabled(_) => 1,
            DevThruportMode::Disabled => 0,
        }
    }

    fn connected_clients_count(&self) -> u32 {
        match &self.mode {
            DevThruportMode::Enabled(transport) => transport.clients.len() as u32,
            DevThruportMode::Disabled => 0,
        }
    }
}

impl RemoteConsoleLinePump for DevThruport {
    fn poll_lines(&mut self, out: &mut Vec<String>) {
        self.poll_remote_lines(out);
    }

    fn send_output_lines(&mut self, lines: &[String]) {
        if let DevThruportMode::Enabled(transport) = &mut self.mode {
            let removed_any = transport.send_output_lines(lines);
            if removed_any {
                self.disconnect_reset_requested = true;
            }
        }
    }

    fn send_thruport_frame(&mut self, line: &str) {
        if let DevThruportMode::Enabled(transport) = &mut self.mode {
            let removed_any = transport.send_frame_line(line);
            if removed_any {
                self.disconnect_reset_requested = true;
            }
        }
    }

    fn status_line(&mut self, telemetry_enabled: bool) -> String {
        let telemetry_value = if telemetry_enabled { 1 } else { 0 };
        format!(
            "thruport.status v1 enabled:{} telemetry:{} clients:{}",
            self.enabled_flag(),
            telemetry_value,
            self.connected_clients_count()
        )
    }

    fn take_disconnect_reset_requested(&mut self) -> bool {
        let was_requested = self.disconnect_reset_requested;
        self.disconnect_reset_requested = false;
        was_requested
    }
}

fn localhost_bind_addr(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn parse_enabled_flag(raw: Option<&str>) -> bool {
    matches!(raw, Some("1"))
}

fn thruport_diag_enabled() -> bool {
    matches!(
        std::env::var(THRUPORT_DIAG_ENV_VAR).ok().as_deref(),
        Some("1")
    )
}

#[cfg(test)]
fn parse_port_or_default(raw: Option<&str>) -> u16 {
    match raw.and_then(|value| value.parse::<u16>().ok()) {
        Some(port) => port,
        None => THRUPORT_DEFAULT_PORT,
    }
}

fn drain_complete_lines(buffer: &mut Vec<u8>, out: &mut Vec<String>) {
    while let Some(newline_index) = buffer.iter().position(|byte| *byte == b'\n') {
        let mut line_bytes = buffer.drain(..=newline_index).collect::<Vec<u8>>();
        line_bytes.pop(); // newline
        if line_bytes.last().copied() == Some(b'\r') {
            line_bytes.pop();
        }

        match String::from_utf8(line_bytes) {
            Ok(line) => {
                if thruport_diag_enabled() {
                    info!(line = %line, "thruport_diag_remote_line_read");
                }
                out.push(line)
            }
            Err(err) => warn!(error = %err, "thruport_invalid_utf8_line_dropped"),
        }
    }
}

fn encode_line_payload(line: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(line.len() + 1);
    payload.extend_from_slice(line.as_bytes());
    payload.push(b'\n');
    payload
}

fn encode_remote_tagged_payload(prefix: &str, line: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(prefix.len() + line.len() + 1);
    payload.extend_from_slice(prefix.as_bytes());
    payload.extend_from_slice(line.as_bytes());
    payload.push(b'\n');
    payload
}

fn ready_line_text(port: u16) -> String {
    format!("thruport.ready v1 port:{port}")
}

fn enqueue_control_line(client: &mut ClientConn, line: &str) {
    enqueue_control_line_with_cap(client, line, MAX_PENDING_CONTROL_BYTES_PER_CLIENT);
}

fn enqueue_control_line_with_cap(client: &mut ClientConn, line: &str, control_cap: usize) {
    let chunk = OutboundChunk {
        class: OutboundClass::Control,
        bytes: encode_remote_tagged_payload(REMOTE_CONTROL_PREFIX, line),
    };
    let chunk_bytes = chunk.bytes.len();
    if chunk_bytes > control_cap {
        if thruport_diag_enabled() {
            info!(
                chunk_bytes,
                control_cap, "thruport_diag_drop_control_chunk_over_cap"
            );
        }
        return;
    }

    while client.queued_control_bytes.saturating_add(chunk_bytes) > control_cap {
        if !evict_oldest_queued_control(client) {
            if thruport_diag_enabled() {
                info!(
                    chunk_bytes,
                    control_cap,
                    queue_len = client.queued_chunks.len(),
                    control_bytes = client.queued_control_bytes,
                    telemetry_bytes = client.queued_telemetry_bytes,
                    "thruport_diag_drop_control_chunk_no_evictable_entry"
                );
            }
            return;
        }
    }

    client.queued_control_bytes = client.queued_control_bytes.saturating_add(chunk_bytes);
    let insert_at = client
        .queued_chunks
        .iter()
        .position(|existing| existing.class == OutboundClass::Telemetry)
        .unwrap_or(client.queued_chunks.len());
    client.queued_chunks.insert(insert_at, chunk);
    if thruport_diag_enabled() {
        info!(
            line = %line,
            queue_len = client.queued_chunks.len(),
            control_bytes = client.queued_control_bytes,
            telemetry_bytes = client.queued_telemetry_bytes,
            "thruport_diag_enqueued_control_line"
        );
    }
}

fn enqueue_telemetry_line_with_cap(client: &mut ClientConn, line: &str, telemetry_cap: usize) {
    let chunk = OutboundChunk {
        class: OutboundClass::Telemetry,
        bytes: encode_remote_tagged_payload(REMOTE_TELEMETRY_PREFIX, line),
    };
    let chunk_bytes = chunk.bytes.len();
    if chunk_bytes > telemetry_cap {
        if thruport_diag_enabled() {
            info!(
                chunk_bytes,
                telemetry_cap, "thruport_diag_drop_telemetry_chunk_over_cap"
            );
        }
        return;
    }

    while client.queued_telemetry_bytes.saturating_add(chunk_bytes) > telemetry_cap {
        if !evict_oldest_queued_telemetry(client) {
            if thruport_diag_enabled() {
                info!(
                    chunk_bytes,
                    telemetry_cap,
                    queue_len = client.queued_chunks.len(),
                    telemetry_bytes = client.queued_telemetry_bytes,
                    "thruport_diag_drop_telemetry_chunk_no_evictable_entry"
                );
            }
            return;
        }
    }

    client.queued_telemetry_bytes = client.queued_telemetry_bytes.saturating_add(chunk_bytes);
    client.queued_chunks.push_back(chunk);
    if thruport_diag_enabled() {
        info!(
            chunk_bytes,
            queue_len = client.queued_chunks.len(),
            control_bytes = client.queued_control_bytes,
            telemetry_bytes = client.queued_telemetry_bytes,
            "thruport_diag_enqueued_telemetry_line"
        );
    }
}

fn evict_oldest_queued_control(client: &mut ClientConn) -> bool {
    let Some(index) = client
        .queued_chunks
        .iter()
        .position(|chunk| chunk.class == OutboundClass::Control)
    else {
        return false;
    };
    let removed = client.queued_chunks.remove(index).expect("index exists");
    client.queued_control_bytes = client
        .queued_control_bytes
        .saturating_sub(removed.bytes.len());
    if thruport_diag_enabled() {
        info!(
            removed_bytes = removed.bytes.len(),
            queue_len = client.queued_chunks.len(),
            control_bytes = client.queued_control_bytes,
            telemetry_bytes = client.queued_telemetry_bytes,
            "thruport_diag_evicted_oldest_control"
        );
    }
    true
}

fn evict_oldest_queued_telemetry(client: &mut ClientConn) -> bool {
    let Some(index) = client
        .queued_chunks
        .iter()
        .position(|chunk| chunk.class == OutboundClass::Telemetry)
    else {
        return false;
    };
    let removed = client.queued_chunks.remove(index).expect("index exists");
    client.queued_telemetry_bytes = client
        .queued_telemetry_bytes
        .saturating_sub(removed.bytes.len());
    if thruport_diag_enabled() {
        info!(
            removed_bytes = removed.bytes.len(),
            queue_len = client.queued_chunks.len(),
            control_bytes = client.queued_control_bytes,
            telemetry_bytes = client.queued_telemetry_bytes,
            "thruport_diag_evicted_oldest_telemetry"
        );
    }
    true
}

fn flush_pending_chunks<F>(
    active_chunk: &mut Option<OutboundChunkState>,
    queued_chunks: &mut VecDeque<OutboundChunk>,
    queued_control_bytes: &mut usize,
    queued_telemetry_bytes: &mut usize,
    mut write_payload: F,
) -> io::Result<()>
where
    F: FnMut(&[u8]) -> io::Result<usize>,
{
    loop {
        if active_chunk.is_none() {
            let Some(chunk) = queued_chunks.pop_front() else {
                return Ok(());
            };
            match chunk.class {
                OutboundClass::Control => {
                    *queued_control_bytes = queued_control_bytes.saturating_sub(chunk.bytes.len());
                }
                OutboundClass::Telemetry => {
                    *queued_telemetry_bytes =
                        queued_telemetry_bytes.saturating_sub(chunk.bytes.len());
                }
            }
            *active_chunk = Some(OutboundChunkState { chunk, written: 0 });
        }

        let state = active_chunk.as_mut().expect("active chunk");
        let remaining = &state.chunk.bytes[state.written..];
        match write_payload(remaining) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "thruport_write_zero",
                ));
            }
            Ok(bytes_written) => {
                state.written = state.written.saturating_add(bytes_written);
                if thruport_diag_enabled() {
                    info!(
                        class = ?state.chunk.class,
                        wrote = bytes_written,
                        written = state.written,
                        total = state.chunk.bytes.len(),
                        queued_len = queued_chunks.len(),
                        queued_control_bytes = *queued_control_bytes,
                        queued_telemetry_bytes = *queued_telemetry_bytes,
                        "thruport_diag_flush_write_progress"
                    );
                }
                if state.written >= state.chunk.bytes.len() {
                    if thruport_diag_enabled() {
                        info!(
                            class = ?state.chunk.class,
                            queued_len = queued_chunks.len(),
                            queued_control_bytes = *queued_control_bytes,
                            queued_telemetry_bytes = *queued_telemetry_bytes,
                            "thruport_diag_flush_chunk_complete"
                        );
                    }
                    *active_chunk = None;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if thruport_diag_enabled() {
                    info!(
                        class = ?state.chunk.class,
                        written = state.written,
                        total = state.chunk.bytes.len(),
                        queued_len = queued_chunks.len(),
                        queued_control_bytes = *queued_control_bytes,
                        queued_telemetry_bytes = *queued_telemetry_bytes,
                        "thruport_diag_flush_would_block"
                    );
                }
                return Ok(());
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use engine::RemoteConsoleLinePump;
    use std::io::{self, Read, Write};
    use std::net::TcpStream;
    use std::thread;
    use std::time::Duration;

    use super::{
        encode_line_payload, enqueue_control_line, enqueue_control_line_with_cap,
        enqueue_telemetry_line_with_cap, flush_pending_chunks, initialize, localhost_bind_addr,
        parse_enabled_flag, parse_port_or_default, ready_line_text, DevThruport, DevThruportConfig,
        DevThruportHooks, DevThruportMode, OutboundChunk, OutboundChunkState, OutboundClass,
        TcpRemoteConsoleTransport, THRUPORT_DEFAULT_PORT,
    };

    fn make_client_conn_for_queue_tests() -> super::ClientConn {
        let listener = std::net::TcpListener::bind(localhost_bind_addr(0)).expect("bind");
        listener
            .set_nonblocking(true)
            .expect("listener nonblocking");
        let addr = listener.local_addr().expect("addr");
        let stream = TcpStream::connect(addr).expect("connect");
        super::ClientConn {
            stream,
            read_buf: Vec::new(),
            active_chunk: None,
            queued_chunks: std::collections::VecDeque::new(),
            queued_control_bytes: 0,
            queued_telemetry_bytes: 0,
        }
    }

    #[test]
    fn initialize_no_op_constructs_without_panic() {
        let hooks = DevThruportHooks::no_op();
        let _thruport = initialize(hooks);
    }

    #[test]
    fn thruport_enablement_from_env_values() {
        assert!(!parse_enabled_flag(None));
        assert!(!parse_enabled_flag(Some("0")));
        assert!(parse_enabled_flag(Some("1")));

        assert_eq!(parse_port_or_default(None), THRUPORT_DEFAULT_PORT);
        assert_eq!(parse_port_or_default(Some("46001")), 46001);
        assert_eq!(
            parse_port_or_default(Some("not-a-port")),
            THRUPORT_DEFAULT_PORT
        );

        let config = DevThruportConfig {
            enabled: parse_enabled_flag(Some("1")),
            port: parse_port_or_default(Some("46002")),
        };
        assert!(config.enabled);
        assert_eq!(config.port, 46002);
    }

    #[test]
    fn bind_address_is_localhost_only() {
        let addr = localhost_bind_addr(46001);
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(addr.port(), 46001);
    }

    #[test]
    fn tcp_transport_receives_newline_delimited_line() {
        let mut transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut client = TcpStream::connect(addr).expect("connect");
        client.write_all(b"help\n").expect("write");
        client.flush().expect("flush");

        let mut out = Vec::new();
        for _ in 0..20 {
            transport.poll_lines(&mut out);
            if !out.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(out, vec!["help".to_string()]);
    }

    #[test]
    fn tcp_transport_writes_output_lines_to_connected_client() {
        let mut transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut client = TcpStream::connect(addr).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set_read_timeout");
        client
            .set_nonblocking(true)
            .expect("set_nonblocking_client");

        let mut ignored = Vec::new();
        for _ in 0..20 {
            transport.poll_lines(&mut ignored);
            if transport.clients.len() == 1 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(transport.clients.len(), 1);

        let expected_ready = format!("C {}\n", ready_line_text(transport.bound_port));
        let expected_ok = "C ok: ready\n";
        let expected_error = "C error: nope\n";
        let mut received = Vec::new();
        for _ in 0..40 {
            transport.send_output_lines(&["ok: ready".to_string(), "error: nope".to_string()]);
            let mut chunk = [0u8; 64];
            match client.read(&mut chunk) {
                Ok(bytes_read) if bytes_read > 0 => {
                    received.extend_from_slice(&chunk[..bytes_read]);
                    let text = String::from_utf8_lossy(&received);
                    if text.contains(&expected_ready)
                        && text.contains(expected_ok)
                        && text.contains(expected_error)
                    {
                        break;
                    }
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => panic!("unexpected read error: {err}"),
            }
            thread::sleep(Duration::from_millis(5));
        }

        let received_text = String::from_utf8_lossy(&received);
        assert!(received_text.contains(&expected_ready));
        assert!(received_text.contains(expected_ok));
        assert!(received_text.contains(expected_error));
    }

    #[test]
    fn disconnect_reset_flag_drains_once_after_client_disconnect() {
        let transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut thruport = DevThruport {
            mode: DevThruportMode::Enabled(transport),
            _hooks: DevThruportHooks::no_op(),
            disconnect_reset_requested: false,
        };

        let client = TcpStream::connect(addr).expect("connect");

        let mut out = Vec::new();
        for _ in 0..20 {
            thruport.poll_lines(&mut out);
            if matches!(&thruport.mode, DevThruportMode::Enabled(t) if t.clients.len() == 1) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        drop(client);

        for _ in 0..20 {
            thruport.poll_lines(&mut out);
            if thruport.take_disconnect_reset_requested() {
                assert!(!thruport.take_disconnect_reset_requested());
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("disconnect reset flag was not set");
    }

    #[test]
    fn disconnect_reset_flag_sets_when_any_client_is_removed() {
        let transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut thruport = DevThruport {
            mode: DevThruportMode::Enabled(transport),
            _hooks: DevThruportHooks::no_op(),
            disconnect_reset_requested: false,
        };

        let client_a = TcpStream::connect(addr).expect("connect a");
        let _client_b = TcpStream::connect(addr).expect("connect b");

        let mut out = Vec::new();
        for _ in 0..30 {
            thruport.poll_lines(&mut out);
            if matches!(&thruport.mode, DevThruportMode::Enabled(t) if t.clients.len() == 2) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        drop(client_a);

        for _ in 0..30 {
            thruport.poll_lines(&mut out);
            if thruport.take_disconnect_reset_requested() {
                assert!(!thruport.take_disconnect_reset_requested());
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("disconnect reset flag was not set for single-client removal");
    }

    #[test]
    fn tcp_transport_writes_telemetry_frame_line_to_connected_client() {
        let transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let bound_port = transport.bound_port;
        let mut thruport = DevThruport {
            mode: DevThruportMode::Enabled(transport),
            _hooks: DevThruportHooks::no_op(),
            disconnect_reset_requested: false,
        };
        let mut client = TcpStream::connect(addr).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set_read_timeout");
        client
            .set_nonblocking(true)
            .expect("set_nonblocking_client");

        let mut ignored = Vec::new();
        for _ in 0..20 {
            thruport.poll_lines(&mut ignored);
            if matches!(&thruport.mode, DevThruportMode::Enabled(t) if t.clients.len() == 1) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let expected_ready = format!("C {}\n", ready_line_text(bound_port));
        let expected = "T thruport.frame v1 tick:1 paused:1 qtick:0 ev:0 in:0 in_bad:0\n";
        let mut received = Vec::new();
        for _ in 0..40 {
            thruport.send_thruport_frame(
                "thruport.frame v1 tick:1 paused:1 qtick:0 ev:0 in:0 in_bad:0",
            );
            let mut chunk = [0u8; 128];
            match client.read(&mut chunk) {
                Ok(bytes_read) if bytes_read > 0 => {
                    received.extend_from_slice(&chunk[..bytes_read]);
                    let text = String::from_utf8_lossy(&received);
                    if text.contains(&expected_ready) && text.contains(expected) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => panic!("unexpected read error: {err}"),
            }
            thread::sleep(Duration::from_millis(5));
        }

        let received_text = String::from_utf8_lossy(&received);
        assert!(received_text.contains(&expected_ready));
        assert!(received_text.contains(expected));
    }

    #[test]
    fn wouldblock_retains_active_chunk_and_queue_order() {
        let mut active_chunk = None;
        let mut queued_chunks = std::collections::VecDeque::new();
        let control_a = OutboundChunk {
            class: OutboundClass::Control,
            bytes: encode_line_payload("ok: sync"),
        };
        let control_b = OutboundChunk {
            class: OutboundClass::Control,
            bytes: encode_line_payload("ok: sim paused"),
        };
        let mut queued_control_bytes = control_a.bytes.len().saturating_add(control_b.bytes.len());
        queued_chunks.push_back(control_a);
        queued_chunks.push_back(control_b);
        let telemetry = OutboundChunk {
            class: OutboundClass::Telemetry,
            bytes: encode_line_payload(
                "thruport.frame v1 tick:1 paused:1 qtick:0 ev:0 in:0 in_bad:0",
            ),
        };
        let mut queued_telemetry_bytes = telemetry.bytes.len();
        queued_chunks.push_back(telemetry);

        let mut first = true;
        let _ = flush_pending_chunks(
            &mut active_chunk,
            &mut queued_chunks,
            &mut queued_control_bytes,
            &mut queued_telemetry_bytes,
            |payload| {
                if first {
                    first = false;
                    Ok(payload.len().min(3))
                } else {
                    Err(io::Error::new(io::ErrorKind::WouldBlock, "blocked"))
                }
            },
        );

        let active = active_chunk.expect("active chunk retained");
        assert_eq!(active.chunk.class, OutboundClass::Control);
        assert!(active.written > 0);
        assert_eq!(queued_chunks.len(), 2);
        assert_eq!(queued_chunks[0].class, OutboundClass::Control);
        assert_eq!(queued_chunks[1].class, OutboundClass::Telemetry);
    }

    #[test]
    fn control_never_dropped_under_telemetry_pressure() {
        let mut client = make_client_conn_for_queue_tests();
        let cap = 64usize;
        for i in 0..50 {
            enqueue_telemetry_line_with_cap(
                &mut client,
                &format!("thruport.frame v1 tick:{i} paused:1 qtick:0 ev:0 in:0 in_bad:0"),
                cap,
            );
        }
        enqueue_control_line(&mut client, "ok: sync");

        let has_control = client
            .queued_chunks
            .iter()
            .any(|chunk| chunk.class == OutboundClass::Control);
        assert!(has_control);
        assert!(client.queued_telemetry_bytes <= cap);
    }

    #[test]
    fn telemetry_eviction_only_affects_telemetry() {
        let mut client = make_client_conn_for_queue_tests();
        let cap = 64usize;
        enqueue_control_line(&mut client, "ok: sync");
        enqueue_control_line(&mut client, "ok: sim paused");
        let controls_before: Vec<Vec<u8>> = client
            .queued_chunks
            .iter()
            .filter(|chunk| chunk.class == OutboundClass::Control)
            .map(|chunk| chunk.bytes.clone())
            .collect();

        for i in 0..80 {
            enqueue_telemetry_line_with_cap(
                &mut client,
                &format!("thruport.frame v1 tick:{i} paused:1 qtick:0 ev:0 in:0 in_bad:0"),
                cap,
            );
        }

        let controls_after: Vec<Vec<u8>> = client
            .queued_chunks
            .iter()
            .filter(|chunk| chunk.class == OutboundClass::Control)
            .map(|chunk| chunk.bytes.clone())
            .collect();
        assert_eq!(controls_before, controls_after);
        assert!(client.queued_telemetry_bytes <= cap);
    }

    #[test]
    fn control_queue_bytes_are_capped() {
        let mut client = make_client_conn_for_queue_tests();
        let cap = 32usize;
        for i in 0..64 {
            enqueue_control_line_with_cap(&mut client, &format!("ctl-{i}"), cap);
        }

        assert!(client.queued_control_bytes <= cap);
        assert!(
            client
                .queued_chunks
                .iter()
                .filter(|chunk| chunk.class == OutboundClass::Control)
                .count()
                > 0
        );
    }

    #[test]
    fn control_eviction_is_fifo_under_pressure() {
        let mut client = make_client_conn_for_queue_tests();
        let cap = 8usize; // fits exactly two control chunks for one-character payloads.
        enqueue_control_line_with_cap(&mut client, "A", cap);
        enqueue_control_line_with_cap(&mut client, "B", cap);
        enqueue_control_line_with_cap(&mut client, "C", cap);

        let controls: Vec<String> = client
            .queued_chunks
            .iter()
            .filter(|chunk| chunk.class == OutboundClass::Control)
            .map(|chunk| String::from_utf8_lossy(&chunk.bytes).to_string())
            .collect();
        assert_eq!(client.queued_control_bytes, cap);
        assert_eq!(controls, vec!["C B\n".to_string(), "C C\n".to_string()]);
    }

    #[test]
    fn poll_flush_drains_without_new_sends() {
        let mut transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut client = TcpStream::connect(addr).expect("connect");
        client
            .set_nonblocking(true)
            .expect("set_nonblocking_client");

        let mut ignored = Vec::new();
        for _ in 0..20 {
            transport.poll_lines(&mut ignored);
            if transport.clients.len() == 1 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(transport.clients.len(), 1);

        transport.clients[0].active_chunk = Some(OutboundChunkState {
            chunk: OutboundChunk {
                class: OutboundClass::Telemetry,
                bytes: encode_line_payload(
                    "thruport.frame v1 tick:1 paused:1 qtick:0 ev:0 in:0 in_bad:0",
                ),
            },
            written: 0,
        });
        enqueue_control_line(&mut transport.clients[0], "ok: sync");

        let mut received = Vec::new();
        for _ in 0..60 {
            transport.poll_lines(&mut ignored);
            let mut chunk = [0u8; 256];
            match client.read(&mut chunk) {
                Ok(bytes_read) if bytes_read > 0 => {
                    received.extend_from_slice(&chunk[..bytes_read]);
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => panic!("unexpected read error: {err}"),
            }

            if String::from_utf8_lossy(&received).contains("C ok: sync\n") {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(String::from_utf8_lossy(&received).contains("C ok: sync\n"));
    }

    #[test]
    fn ready_line_is_sent_immediately_on_accept() {
        let mut transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut client = TcpStream::connect(addr).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set_read_timeout");
        client
            .set_nonblocking(true)
            .expect("set_nonblocking_client");

        let expected = format!("C {}\n", ready_line_text(transport.bound_port));
        let mut out = Vec::new();
        let mut received = Vec::new();
        for _ in 0..40 {
            transport.poll_lines(&mut out);
            let mut chunk = [0u8; 128];
            match client.read(&mut chunk) {
                Ok(bytes_read) if bytes_read > 0 => {
                    received.extend_from_slice(&chunk[..bytes_read]);
                    if String::from_utf8_lossy(&received).contains(&expected) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => panic!("unexpected read error: {err}"),
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(String::from_utf8_lossy(&received).contains(&expected));
    }

    #[test]
    fn status_line_reports_enabled_telemetry_and_client_count() {
        let transport = TcpRemoteConsoleTransport::bind_localhost(0).expect("bind");
        let addr = transport.listener.local_addr().expect("local_addr");
        let mut thruport = DevThruport {
            mode: DevThruportMode::Enabled(transport),
            _hooks: DevThruportHooks::no_op(),
            disconnect_reset_requested: false,
        };
        let _client = TcpStream::connect(addr).expect("connect");

        let mut out = Vec::new();
        for _ in 0..20 {
            thruport.poll_lines(&mut out);
            if matches!(&thruport.mode, DevThruportMode::Enabled(t) if t.clients.len() == 1) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(
            thruport.status_line(true),
            "thruport.status v1 enabled:1 telemetry:1 clients:1"
        );
    }

    #[test]
    fn telemetry_pressure_preserves_all_control_lines_in_order_under_strain() {
        let mut client = make_client_conn_for_queue_tests();
        let cap = 96usize;
        let control_payloads = vec![
            "ok: sync",
            "ok: queued tick 1",
            "ok: sim paused",
            "error: test fault",
            "ok: sim resumed",
        ];

        for i in 0..300 {
            enqueue_telemetry_line_with_cap(
                &mut client,
                &format!("thruport.frame v1 tick:{i} paused:1 qtick:0 ev:0 in:0 in_bad:0"),
                cap,
            );
            if i % 57 == 0 {
                let idx = (i / 57) as usize;
                if idx < control_payloads.len() {
                    enqueue_control_line(&mut client, control_payloads[idx]);
                }
            }
        }

        for payload in &control_payloads {
            enqueue_control_line(&mut client, payload);
        }

        let queued_controls: Vec<Vec<u8>> = client
            .queued_chunks
            .iter()
            .filter(|chunk| chunk.class == OutboundClass::Control)
            .map(|chunk| chunk.bytes.clone())
            .collect();
        let expected_controls: Vec<Vec<u8>> = control_payloads
            .iter()
            .map(|payload| format!("C {payload}\n").into_bytes())
            .collect();

        assert!(client.queued_telemetry_bytes <= cap);
        assert!(queued_controls.len() >= control_payloads.len());
        assert_eq!(
            queued_controls[queued_controls.len() - control_payloads.len()..],
            expected_controls
        );
    }

    #[test]
    fn flush_pending_chunks_handles_large_partial_write_sequence() {
        let mut active_chunk = None;
        let mut queued_chunks = std::collections::VecDeque::new();
        let mut queued_control_bytes = 0usize;
        let mut queued_telemetry_bytes = 0usize;
        for i in 0..240 {
            if i % 6 == 0 {
                let chunk = OutboundChunk {
                    class: OutboundClass::Control,
                    bytes: encode_line_payload(&format!("ok: batch-{i}")),
                };
                queued_control_bytes = queued_control_bytes.saturating_add(chunk.bytes.len());
                queued_chunks.push_back(chunk);
            } else {
                let chunk = OutboundChunk {
                    class: OutboundClass::Telemetry,
                    bytes: encode_line_payload(&format!(
                        "thruport.frame v1 tick:{i} paused:1 qtick:0 ev:0 in:0 in_bad:0"
                    )),
                };
                queued_telemetry_bytes = queued_telemetry_bytes.saturating_add(chunk.bytes.len());
                queued_chunks.push_back(chunk);
            }
        }

        let mut stride = 1usize;
        for _ in 0..20_000 {
            flush_pending_chunks(
                &mut active_chunk,
                &mut queued_chunks,
                &mut queued_control_bytes,
                &mut queued_telemetry_bytes,
                |payload| {
                    let step = stride.min(payload.len());
                    stride = if stride >= 7 { 1 } else { stride + 1 };
                    Ok(step)
                },
            )
            .expect("flush should succeed");
            if active_chunk.is_none() && queued_chunks.is_empty() {
                break;
            }
        }

        assert!(active_chunk.is_none());
        assert!(queued_chunks.is_empty());
        assert_eq!(queued_control_bytes, 0);
        assert_eq!(queued_telemetry_bytes, 0);
    }
}
