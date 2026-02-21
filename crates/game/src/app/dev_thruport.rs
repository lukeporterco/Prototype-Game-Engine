use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use engine::RemoteConsoleLinePump;
use tracing::warn;

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
const THRUPORT_DEFAULT_PORT: u16 = 46001;

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
    clients: Vec<ClientConn>,
}

#[derive(Debug)]
struct ClientConn {
    stream: TcpStream,
    read_buf: Vec<u8>,
}

impl TcpRemoteConsoleTransport {
    fn bind_localhost(port: u16) -> io::Result<Self> {
        let addr = localhost_bind_addr(port);
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            clients: Vec::new(),
        })
    }

    fn poll_lines(&mut self, out: &mut Vec<String>) {
        self.accept_pending_clients();
        self.poll_client_lines(out);
    }

    fn accept_pending_clients(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((stream, _addr)) => {
                    if let Err(err) = stream.set_nonblocking(true) {
                        warn!(error = %err, "thruport_client_nonblocking_failed");
                        continue;
                    }
                    self.clients.push(ClientConn {
                        stream,
                        read_buf: Vec::new(),
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => {
                    warn!(error = %err, "thruport_accept_failed");
                    break;
                }
            }
        }
    }

    fn poll_client_lines(&mut self, out: &mut Vec<String>) {
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
            } else {
                index += 1;
            }
        }
    }

    fn send_output_lines(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }

        let mut index = 0usize;
        while index < self.clients.len() {
            let mut remove_client = false;
            {
                let client = &mut self.clients[index];
                for line in lines {
                    if let Err(err) = write_line_non_blocking(&mut client.stream, line) {
                        if err.kind() == io::ErrorKind::WouldBlock {
                            break;
                        }

                        warn!(error = %err, "thruport_client_write_failed");
                        remove_client = true;
                        break;
                    }
                }
            }

            if remove_client {
                self.clients.swap_remove(index);
            } else {
                index += 1;
            }
        }
    }
}

pub(crate) fn initialize(hooks: DevThruportHooks) -> DevThruport {
    exercise_forward_contracts_noop();

    let config = DevThruportConfig::from_env();
    let mode = if config.enabled {
        match TcpRemoteConsoleTransport::bind_localhost(config.port) {
            Ok(transport) => DevThruportMode::Enabled(transport),
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
            let clients_before = transport.clients.len();
            transport.poll_lines(out);
            if clients_before > 0 && transport.clients.is_empty() {
                self.disconnect_reset_requested = true;
            }
        }
    }
}

impl RemoteConsoleLinePump for DevThruport {
    fn poll_lines(&mut self, out: &mut Vec<String>) {
        self.poll_remote_lines(out);
    }

    fn send_output_lines(&mut self, lines: &[String]) {
        if let DevThruportMode::Enabled(transport) = &mut self.mode {
            let clients_before = transport.clients.len();
            transport.send_output_lines(lines);
            if clients_before > 0 && transport.clients.is_empty() {
                self.disconnect_reset_requested = true;
            }
        }
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
            Ok(line) => out.push(line),
            Err(err) => warn!(error = %err, "thruport_invalid_utf8_line_dropped"),
        }
    }
}

fn write_line_non_blocking(stream: &mut TcpStream, line: &str) -> io::Result<()> {
    let mut payload = Vec::with_capacity(line.len() + 1);
    payload.extend_from_slice(line.as_bytes());
    payload.push(b'\n');

    let mut written = 0usize;
    while written < payload.len() {
        match stream.write(&payload[written..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "thruport_write_zero",
                ));
            }
            Ok(bytes_written) => {
                written += bytes_written;
            }
            Err(err) => return Err(err),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use engine::RemoteConsoleLinePump;
    use std::io::{self, Read, Write};
    use std::net::TcpStream;
    use std::thread;
    use std::time::Duration;

    use super::{
        initialize, localhost_bind_addr, parse_enabled_flag, parse_port_or_default, DevThruport,
        DevThruportConfig, DevThruportHooks, DevThruportMode, TcpRemoteConsoleTransport,
        THRUPORT_DEFAULT_PORT,
    };

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

        let expected = b"ok: ready\nerror: nope\n";
        let mut received = Vec::new();
        for _ in 0..40 {
            transport.send_output_lines(&["ok: ready".to_string(), "error: nope".to_string()]);
            let mut chunk = [0u8; 64];
            match client.read(&mut chunk) {
                Ok(bytes_read) if bytes_read > 0 => {
                    received.extend_from_slice(&chunk[..bytes_read]);
                    if received.ends_with(expected) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => panic!("unexpected read error: {err}"),
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(received.ends_with(expected));
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
}
