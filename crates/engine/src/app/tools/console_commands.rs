use std::collections::{HashMap, VecDeque};

use crate::app::{FloorId, SceneKey};

use super::ConsoleState;

const MAX_PENDING_DEBUG_COMMANDS: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DebugCommand {
    ResetScene,
    Sync,
    ThruportStatus,
    ThruportTelemetry {
        enabled: bool,
    },
    PauseSim,
    ResumeSim,
    Tick {
        steps: u32,
    },
    DumpState,
    DumpAi,
    ScenarioSetup {
        scenario_id: String,
    },
    FloorSet {
        floor: FloorId,
    },
    SwitchScene {
        scene: SceneKey,
    },
    Quit,
    Despawn {
        entity_id: u64,
    },
    Spawn {
        def_name: String,
        position: Option<(f32, f32)>,
    },
    Select {
        entity_id: u64,
    },
    OrderMove {
        x: f32,
        y: f32,
    },
    OrderInteract {
        target_entity_id: u64,
    },
    InjectInput {
        event: InjectedInputEvent,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InjectedInputEvent {
    KeyDown { key: InjectedKey },
    KeyUp { key: InjectedKey },
    MouseMove { x: f32, y: f32 },
    MouseDown { button: InjectedMouseButton },
    MouseUp { button: InjectedMouseButton },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InjectedKey {
    W,
    A,
    S,
    D,
    Up,
    Down,
    Left,
    Right,
    I,
    J,
    K,
    L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InjectedMouseButton {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocalAction {
    Help,
    Clear,
    Echo { text: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ParsedCommand {
    Local(LocalAction),
    Queueable(DebugCommand),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommandParseError {
    reason: String,
    usage: String,
}

type ParseFn = dyn Fn(&[String]) -> Result<ParsedCommand, CommandParseError> + Send + Sync;

pub(crate) struct CommandSpec {
    name: String,
    help: String,
    arg_schema: String,
    parse: Box<ParseFn>,
}

pub(crate) struct ConsoleCommandRegistry {
    specs: Vec<CommandSpec>,
    lookup_by_lower_name: HashMap<String, usize>,
}

impl ConsoleCommandRegistry {
    pub(crate) fn new() -> Self {
        Self {
            specs: Vec::new(),
            lookup_by_lower_name: HashMap::new(),
        }
    }

    pub(crate) fn with_engine_builtins() -> Self {
        let mut registry = Self::new();
        registry
            .register("help", "List commands", "", parse_help_command)
            .expect("built-in command registration should not fail");
        registry
            .register("clear", "Clear console output", "", parse_clear_command)
            .expect("built-in command registration should not fail");
        registry
            .register(
                "echo",
                "Print text to console",
                "<text...>",
                parse_echo_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "reset_scene",
                "Reset active scene",
                "",
                parse_reset_scene_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "sync",
                "Flush queued command processing barrier",
                "",
                parse_sync_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "thruport.status",
                "Dump thruport transport status",
                "",
                parse_thruport_status_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "thruport.telemetry",
                "Toggle thruport telemetry output",
                "<on|off>",
                parse_thruport_telemetry_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "pause_sim",
                "Pause simulation stepping",
                "",
                parse_pause_sim_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "resume_sim",
                "Resume simulation stepping",
                "",
                parse_resume_sim_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "tick",
                "Advance simulation by fixed ticks",
                "<steps:u32>",
                parse_tick_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "dump.state",
                "Dump deterministic state probe",
                "",
                parse_dump_state_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "dump.ai",
                "Dump deterministic AI probe",
                "",
                parse_dump_ai_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "scenario.setup",
                "Setup deterministic gameplay scenario",
                "<scenario_id:string>",
                parse_scenario_setup_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "floor.set",
                "Set active floor",
                "<rooftop|main|basement>",
                parse_floor_set_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "switch_scene",
                "Switch active scene",
                "<scene_id:a|b>",
                parse_switch_scene_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register("quit", "Quit app", "", parse_quit_command)
            .expect("built-in command registration should not fail");
        registry
            .register(
                "despawn",
                "Despawn entity by id",
                "<entity_id:u64>",
                parse_despawn_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "spawn",
                "Spawn entity by def name",
                "<def_name:string> [x:f32 y:f32]",
                parse_spawn_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "select",
                "Select entity by id",
                "<entity_id:u64>",
                parse_select_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "order.move",
                "Queue move order for selected actor",
                "<x:f32> <y:f32>",
                parse_order_move_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "order.interact",
                "Queue interaction order for selected actor",
                "<target_entity_id:u64>",
                parse_order_interact_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "input.key_down",
                "Inject key down",
                "<key:w|a|s|d|up|down|left|right|i|j|k|l>",
                parse_input_key_down_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "input.key_up",
                "Inject key up",
                "<key:w|a|s|d|up|down|left|right|i|j|k|l>",
                parse_input_key_up_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "input.mouse_move",
                "Inject mouse move (px)",
                "<x:f32> <y:f32>",
                parse_input_mouse_move_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "input.mouse_down",
                "Inject mouse down",
                "<button:left|right>",
                parse_input_mouse_down_command,
            )
            .expect("built-in command registration should not fail");
        registry
            .register(
                "input.mouse_up",
                "Inject mouse up",
                "<button:left|right>",
                parse_input_mouse_up_command,
            )
            .expect("built-in command registration should not fail");
        registry
    }

    pub(crate) fn register<F>(
        &mut self,
        name: impl Into<String>,
        help: impl Into<String>,
        arg_schema: impl Into<String>,
        parse: F,
    ) -> Result<(), String>
    where
        F: Fn(&[String]) -> Result<ParsedCommand, CommandParseError> + Send + Sync + 'static,
    {
        let name = name.into();
        if name.trim().is_empty() {
            return Err("command name cannot be empty".to_string());
        }
        let lower = name.to_ascii_lowercase();
        if self.lookup_by_lower_name.contains_key(&lower) {
            return Err(format!("duplicate command registration: {name}"));
        }

        self.specs.push(CommandSpec {
            name,
            help: help.into(),
            arg_schema: arg_schema.into(),
            parse: Box::new(parse),
        });
        self.lookup_by_lower_name
            .insert(lower, self.specs.len() - 1);
        Ok(())
    }

    pub(crate) fn lookup(&self, input_name: &str) -> Option<&CommandSpec> {
        let lower = input_name.to_ascii_lowercase();
        let index = self.lookup_by_lower_name.get(&lower)?;
        self.specs.get(*index)
    }

    pub(crate) fn iter_specs_in_order(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        // Help output order is registration order by contract.
        self.specs.iter().map(|spec| {
            (
                spec.name.as_str(),
                spec.help.as_str(),
                spec.arg_schema.as_str(),
            )
        })
    }
}

pub(crate) struct ConsoleCommandProcessor {
    registry: ConsoleCommandRegistry,
    pending_debug_commands: VecDeque<DebugCommand>,
}

impl Default for ConsoleCommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleCommandProcessor {
    pub(crate) fn new() -> Self {
        Self {
            registry: ConsoleCommandRegistry::with_engine_builtins(),
            pending_debug_commands: VecDeque::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn registry_mut(&mut self) -> &mut ConsoleCommandRegistry {
        &mut self.registry
    }

    pub(crate) fn process_pending_lines(&mut self, console: &mut ConsoleState) {
        let mut lines = Vec::new();
        console.drain_pending_lines_into(&mut lines);

        for raw_line in lines {
            self.process_line(console, &raw_line);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn drain_pending_debug_commands_into(&mut self, out: &mut Vec<DebugCommand>) {
        out.extend(self.pending_debug_commands.drain(..));
    }

    fn process_line(&mut self, console: &mut ConsoleState, raw_line: &str) {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            return;
        }

        let tokens = match tokenize_line(trimmed) {
            Ok(tokens) => tokens,
            Err(reason) => {
                console.append_output_line(format!("error: {reason}. usage: help"));
                return;
            }
        };
        if tokens.is_empty() {
            return;
        }

        let command_name = &tokens[0];
        let args = &tokens[1..];
        let Some(spec) = self.registry.lookup(command_name) else {
            console.append_output_line(format!(
                "error: unknown command '{}'. try: help",
                command_name
            ));
            return;
        };

        match (spec.parse)(args) {
            Ok(ParsedCommand::Local(action)) => self.apply_local_action(console, action),
            Ok(ParsedCommand::Queueable(command)) => self.push_queueable(command),
            Err(error) => {
                console
                    .append_output_line(format!("error: {}. usage: {}", error.reason, error.usage));
            }
        }
    }

    fn apply_local_action(&self, console: &mut ConsoleState, action: LocalAction) {
        match action {
            LocalAction::Help => {
                for (name, help, arg_schema) in self.registry.iter_specs_in_order() {
                    let line = if arg_schema.is_empty() {
                        format!("{name} - {help}")
                    } else {
                        format!("{name} {arg_schema} - {help}")
                    };
                    console.append_output_line(line);
                }
            }
            LocalAction::Clear => {
                console.clear_output_lines();
            }
            LocalAction::Echo { text } => {
                console.append_output_line(text);
            }
        }
    }

    fn push_queueable(&mut self, command: DebugCommand) {
        if self.pending_debug_commands.len() == MAX_PENDING_DEBUG_COMMANDS {
            self.pending_debug_commands.pop_front();
        }
        self.pending_debug_commands.push_back(command);
    }
}

fn tokenize_line(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut seen_token_content = false;
    let mut just_closed_quote = false;

    for ch in line.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                seen_token_content = true;
                if !in_quotes {
                    just_closed_quote = true;
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if seen_token_content || just_closed_quote || !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    seen_token_content = false;
                    just_closed_quote = false;
                }
            }
            _ => {
                current.push(ch);
                seen_token_content = true;
                just_closed_quote = false;
            }
        }
    }

    if in_quotes {
        return Err("unterminated quoted string".to_string());
    }

    if seen_token_content || just_closed_quote || !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

fn parse_help_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "help")?;
    Ok(ParsedCommand::Local(LocalAction::Help))
}

fn parse_clear_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "clear")?;
    Ok(ParsedCommand::Local(LocalAction::Clear))
}

fn parse_echo_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.is_empty() {
        return Err(CommandParseError {
            reason: "missing required argument <text...>".to_string(),
            usage: "echo <text...>".to_string(),
        });
    }
    Ok(ParsedCommand::Local(LocalAction::Echo {
        text: args.join(" "),
    }))
}

fn parse_reset_scene_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "reset_scene")?;
    Ok(ParsedCommand::Queueable(DebugCommand::ResetScene))
}

fn parse_sync_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "sync")?;
    Ok(ParsedCommand::Queueable(DebugCommand::Sync))
}

fn parse_thruport_status_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "thruport.status")?;
    Ok(ParsedCommand::Queueable(DebugCommand::ThruportStatus))
}

fn parse_thruport_telemetry_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <on|off>".to_string(),
            usage: "thruport.telemetry <on|off>".to_string(),
        });
    }

    let enabled = match args[0].to_ascii_lowercase().as_str() {
        "on" => true,
        "off" => false,
        _ => {
            return Err(CommandParseError {
                reason: format!("invalid value '{}' (expected on|off)", args[0]),
                usage: "thruport.telemetry <on|off>".to_string(),
            });
        }
    };

    Ok(ParsedCommand::Queueable(DebugCommand::ThruportTelemetry {
        enabled,
    }))
}

fn parse_pause_sim_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "pause_sim")?;
    Ok(ParsedCommand::Queueable(DebugCommand::PauseSim))
}

fn parse_resume_sim_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "resume_sim")?;
    Ok(ParsedCommand::Queueable(DebugCommand::ResumeSim))
}

fn parse_tick_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <steps>".to_string(),
            usage: "tick <steps>".to_string(),
        });
    }

    let steps = args[0].parse::<u32>().map_err(|_| CommandParseError {
        reason: format!("invalid steps '{}' (expected u32 > 0)", args[0]),
        usage: "tick <steps>".to_string(),
    })?;
    if steps == 0 {
        return Err(CommandParseError {
            reason: "steps must be > 0".to_string(),
            usage: "tick <steps>".to_string(),
        });
    }

    Ok(ParsedCommand::Queueable(DebugCommand::Tick { steps }))
}

fn parse_dump_state_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "dump.state")?;
    Ok(ParsedCommand::Queueable(DebugCommand::DumpState))
}

fn parse_dump_ai_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "dump.ai")?;
    Ok(ParsedCommand::Queueable(DebugCommand::DumpAi))
}

fn parse_scenario_setup_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <scenario_id>".to_string(),
            usage: "scenario.setup <scenario_id>".to_string(),
        });
    }
    Ok(ParsedCommand::Queueable(DebugCommand::ScenarioSetup {
        scenario_id: args[0].clone(),
    }))
}

fn parse_floor_set_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <rooftop|main|basement>".to_string(),
            usage: "floor.set <rooftop|main|basement>".to_string(),
        });
    }

    let floor = match args[0].to_ascii_lowercase().as_str() {
        "rooftop" => FloorId::Rooftop,
        "main" => FloorId::Main,
        "basement" => FloorId::Basement,
        _ => {
            return Err(CommandParseError {
                reason: format!(
                    "invalid floor '{}' (expected rooftop|main|basement)",
                    args[0]
                ),
                usage: "floor.set <rooftop|main|basement>".to_string(),
            });
        }
    };

    Ok(ParsedCommand::Queueable(DebugCommand::FloorSet { floor }))
}

fn parse_switch_scene_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <scene_id>".to_string(),
            usage: "switch_scene <scene_id>".to_string(),
        });
    }

    let scene = match args[0].to_ascii_lowercase().as_str() {
        "a" => SceneKey::A,
        "b" => SceneKey::B,
        _ => {
            return Err(CommandParseError {
                reason: format!("unknown scene id '{}' (expected a|b)", args[0]),
                usage: "switch_scene <scene_id>".to_string(),
            });
        }
    };

    Ok(ParsedCommand::Queueable(DebugCommand::SwitchScene {
        scene,
    }))
}

fn parse_quit_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    require_no_args(args, "quit")?;
    Ok(ParsedCommand::Queueable(DebugCommand::Quit))
}

fn parse_despawn_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <entity_id>".to_string(),
            usage: "despawn <entity_id>".to_string(),
        });
    }

    let entity_id = args[0].parse::<u64>().map_err(|_| CommandParseError {
        reason: format!("invalid entity id '{}' (expected u64)", args[0]),
        usage: "despawn <entity_id>".to_string(),
    })?;

    Ok(ParsedCommand::Queueable(DebugCommand::Despawn {
        entity_id,
    }))
}

fn parse_spawn_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 && args.len() != 3 {
        return Err(CommandParseError {
            reason: "expected <def_name> or <def_name> <x> <y>".to_string(),
            usage: "spawn <def_name> [x y]".to_string(),
        });
    }

    let def_name = args[0].clone();
    let position = if args.len() == 3 {
        let x = args[1].parse::<f32>().map_err(|_| CommandParseError {
            reason: format!("invalid x coordinate '{}' (expected f32)", args[1]),
            usage: "spawn <def_name> [x y]".to_string(),
        })?;
        let y = args[2].parse::<f32>().map_err(|_| CommandParseError {
            reason: format!("invalid y coordinate '{}' (expected f32)", args[2]),
            usage: "spawn <def_name> [x y]".to_string(),
        })?;
        Some((x, y))
    } else {
        None
    };

    Ok(ParsedCommand::Queueable(DebugCommand::Spawn {
        def_name,
        position,
    }))
}

fn parse_select_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <entity_id>".to_string(),
            usage: "select <entity_id>".to_string(),
        });
    }

    let entity_id = args[0].parse::<u64>().map_err(|_| CommandParseError {
        reason: format!("invalid entity id '{}' (expected u64)", args[0]),
        usage: "select <entity_id>".to_string(),
    })?;

    Ok(ParsedCommand::Queueable(DebugCommand::Select { entity_id }))
}

fn parse_order_move_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 2 {
        return Err(CommandParseError {
            reason: "expected exactly two arguments <x> <y>".to_string(),
            usage: "order.move <x> <y>".to_string(),
        });
    }

    let x = args[0].parse::<f32>().map_err(|_| CommandParseError {
        reason: format!("invalid x coordinate '{}' (expected f32)", args[0]),
        usage: "order.move <x> <y>".to_string(),
    })?;
    let y = args[1].parse::<f32>().map_err(|_| CommandParseError {
        reason: format!("invalid y coordinate '{}' (expected f32)", args[1]),
        usage: "order.move <x> <y>".to_string(),
    })?;

    Ok(ParsedCommand::Queueable(DebugCommand::OrderMove { x, y }))
}

fn parse_order_interact_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <target_entity_id>".to_string(),
            usage: "order.interact <target_entity_id>".to_string(),
        });
    }

    let target_entity_id = args[0].parse::<u64>().map_err(|_| CommandParseError {
        reason: format!("invalid target entity id '{}' (expected u64)", args[0]),
        usage: "order.interact <target_entity_id>".to_string(),
    })?;

    Ok(ParsedCommand::Queueable(DebugCommand::OrderInteract {
        target_entity_id,
    }))
}

fn parse_input_key_down_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    let key = parse_single_injected_key_arg(args, "input.key_down <key>")?;
    Ok(ParsedCommand::Queueable(DebugCommand::InjectInput {
        event: InjectedInputEvent::KeyDown { key },
    }))
}

fn parse_input_key_up_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    let key = parse_single_injected_key_arg(args, "input.key_up <key>")?;
    Ok(ParsedCommand::Queueable(DebugCommand::InjectInput {
        event: InjectedInputEvent::KeyUp { key },
    }))
}

fn parse_input_mouse_move_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    if args.len() != 2 {
        return Err(CommandParseError {
            reason: "expected exactly two arguments <x> <y>".to_string(),
            usage: "input.mouse_move <x> <y>".to_string(),
        });
    }

    let x = args[0].parse::<f32>().map_err(|_| CommandParseError {
        reason: format!("invalid x coordinate '{}' (expected f32)", args[0]),
        usage: "input.mouse_move <x> <y>".to_string(),
    })?;
    let y = args[1].parse::<f32>().map_err(|_| CommandParseError {
        reason: format!("invalid y coordinate '{}' (expected f32)", args[1]),
        usage: "input.mouse_move <x> <y>".to_string(),
    })?;

    Ok(ParsedCommand::Queueable(DebugCommand::InjectInput {
        event: InjectedInputEvent::MouseMove { x, y },
    }))
}

fn parse_input_mouse_down_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    let button = parse_single_injected_button_arg(args, "input.mouse_down <button>")?;
    Ok(ParsedCommand::Queueable(DebugCommand::InjectInput {
        event: InjectedInputEvent::MouseDown { button },
    }))
}

fn parse_input_mouse_up_command(args: &[String]) -> Result<ParsedCommand, CommandParseError> {
    let button = parse_single_injected_button_arg(args, "input.mouse_up <button>")?;
    Ok(ParsedCommand::Queueable(DebugCommand::InjectInput {
        event: InjectedInputEvent::MouseUp { button },
    }))
}

fn parse_single_injected_key_arg(
    args: &[String],
    usage: &str,
) -> Result<InjectedKey, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <key>".to_string(),
            usage: usage.to_string(),
        });
    }
    parse_injected_key(&args[0], usage)
}

fn parse_single_injected_button_arg(
    args: &[String],
    usage: &str,
) -> Result<InjectedMouseButton, CommandParseError> {
    if args.len() != 1 {
        return Err(CommandParseError {
            reason: "expected exactly one argument <button>".to_string(),
            usage: usage.to_string(),
        });
    }
    parse_injected_mouse_button(&args[0], usage)
}

fn parse_injected_key(raw: &str, usage: &str) -> Result<InjectedKey, CommandParseError> {
    let lower = raw.to_ascii_lowercase();
    let key = match lower.as_str() {
        "w" => InjectedKey::W,
        "a" => InjectedKey::A,
        "s" => InjectedKey::S,
        "d" => InjectedKey::D,
        "up" => InjectedKey::Up,
        "down" => InjectedKey::Down,
        "left" => InjectedKey::Left,
        "right" => InjectedKey::Right,
        "i" => InjectedKey::I,
        "j" => InjectedKey::J,
        "k" => InjectedKey::K,
        "l" => InjectedKey::L,
        _ => {
            return Err(CommandParseError {
                reason: format!(
                    "invalid key '{}' (expected w|a|s|d|up|down|left|right|i|j|k|l)",
                    raw
                ),
                usage: usage.to_string(),
            });
        }
    };
    Ok(key)
}

fn parse_injected_mouse_button(
    raw: &str,
    usage: &str,
) -> Result<InjectedMouseButton, CommandParseError> {
    let lower = raw.to_ascii_lowercase();
    let button = match lower.as_str() {
        "left" => InjectedMouseButton::Left,
        "right" => InjectedMouseButton::Right,
        _ => {
            return Err(CommandParseError {
                reason: format!("invalid button '{}' (expected left|right)", raw),
                usage: usage.to_string(),
            });
        }
    };
    Ok(button)
}

fn require_no_args(args: &[String], usage: &str) -> Result<(), CommandParseError> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(CommandParseError {
            reason: "unexpected extra arguments".to_string(),
            usage: usage.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_output(console: &ConsoleState) -> Vec<String> {
        console.output_lines().map(ToString::to_string).collect()
    }

    #[test]
    fn help_lists_commands_in_registration_order() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("help");

        processor.process_pending_lines(&mut console);
        let lines = collect_output(&console);

        assert_eq!(lines[0], "help - List commands");
        assert_eq!(lines[1], "clear - Clear console output");
        assert_eq!(lines[2], "echo <text...> - Print text to console");
        assert_eq!(lines[3], "reset_scene - Reset active scene");
        assert_eq!(lines[4], "sync - Flush queued command processing barrier");
        assert_eq!(lines[5], "thruport.status - Dump thruport transport status");
        assert_eq!(
            lines[6],
            "thruport.telemetry <on|off> - Toggle thruport telemetry output"
        );
        assert_eq!(lines[7], "pause_sim - Pause simulation stepping");
        assert_eq!(lines[8], "resume_sim - Resume simulation stepping");
        assert_eq!(
            lines[9],
            "tick <steps:u32> - Advance simulation by fixed ticks"
        );
        assert_eq!(lines[10], "dump.state - Dump deterministic state probe");
        assert_eq!(lines[11], "dump.ai - Dump deterministic AI probe");
        assert_eq!(
            lines[12],
            "scenario.setup <scenario_id:string> - Setup deterministic gameplay scenario"
        );
        assert_eq!(
            lines[13],
            "floor.set <rooftop|main|basement> - Set active floor"
        );
        assert_eq!(
            lines[14],
            "switch_scene <scene_id:a|b> - Switch active scene"
        );
        assert_eq!(lines[15], "quit - Quit app");
        assert_eq!(lines[16], "despawn <entity_id:u64> - Despawn entity by id");
        assert_eq!(
            lines[17],
            "spawn <def_name:string> [x:f32 y:f32] - Spawn entity by def name"
        );
        assert_eq!(lines[18], "select <entity_id:u64> - Select entity by id");
        assert_eq!(
            lines[19],
            "order.move <x:f32> <y:f32> - Queue move order for selected actor"
        );
        assert_eq!(
            lines[20],
            "order.interact <target_entity_id:u64> - Queue interaction order for selected actor"
        );
        assert_eq!(
            lines[21],
            "input.key_down <key:w|a|s|d|up|down|left|right|i|j|k|l> - Inject key down"
        );
        assert_eq!(
            lines[22],
            "input.key_up <key:w|a|s|d|up|down|left|right|i|j|k|l> - Inject key up"
        );
        assert_eq!(
            lines[23],
            "input.mouse_move <x:f32> <y:f32> - Inject mouse move (px)"
        );
        assert_eq!(
            lines[24],
            "input.mouse_down <button:left|right> - Inject mouse down"
        );
        assert_eq!(
            lines[25],
            "input.mouse_up <button:left|right> - Inject mouse up"
        );
    }

    #[test]
    fn unknown_command_reports_clear_error() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("nope");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec!["error: unknown command 'nope'. try: help"]
        );
    }

    #[test]
    fn bad_args_report_usage_hint() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("despawn foo");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec!["error: invalid entity id 'foo' (expected u64). usage: despawn <entity_id>"]
        );
    }

    #[test]
    fn local_commands_are_immediate_and_not_enqueued() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("echo hi");
        console.push_pending_line_for_test("clear");
        console.push_pending_line_for_test("echo bye");

        processor.process_pending_lines(&mut console);

        let mut queued = Vec::new();
        processor.drain_pending_debug_commands_into(&mut queued);
        assert!(queued.is_empty());
        assert_eq!(collect_output(&console), vec!["bye"]);
    }

    #[test]
    fn queueable_parse_success_enqueues_debug_command() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("reset_scene");
        console.push_pending_line_for_test("sync");
        console.push_pending_line_for_test("thruport.status");
        console.push_pending_line_for_test("thruport.telemetry on");
        console.push_pending_line_for_test("thruport.telemetry off");
        console.push_pending_line_for_test("pause_sim");
        console.push_pending_line_for_test("tick 5");
        console.push_pending_line_for_test("resume_sim");
        console.push_pending_line_for_test("dump.state");
        console.push_pending_line_for_test("dump.ai");
        console.push_pending_line_for_test("scenario.setup combat_chaser");
        console.push_pending_line_for_test("floor.set basement");
        console.push_pending_line_for_test("switch_scene a");
        console.push_pending_line_for_test("quit");
        console.push_pending_line_for_test("despawn 42");
        console.push_pending_line_for_test("spawn proto.worker 1.5 -2.0");
        console.push_pending_line_for_test("select 43");
        console.push_pending_line_for_test("order.move 4.0 -8.0");
        console.push_pending_line_for_test("order.interact 44");
        console.push_pending_line_for_test("input.key_down w");
        console.push_pending_line_for_test("input.key_up right");
        console.push_pending_line_for_test("input.mouse_move 10.0 -4.0");
        console.push_pending_line_for_test("input.mouse_down left");
        console.push_pending_line_for_test("input.mouse_up right");

        processor.process_pending_lines(&mut console);

        let mut queued = Vec::new();
        processor.drain_pending_debug_commands_into(&mut queued);
        assert_eq!(
            queued,
            vec![
                DebugCommand::ResetScene,
                DebugCommand::Sync,
                DebugCommand::ThruportStatus,
                DebugCommand::ThruportTelemetry { enabled: true },
                DebugCommand::ThruportTelemetry { enabled: false },
                DebugCommand::PauseSim,
                DebugCommand::Tick { steps: 5 },
                DebugCommand::ResumeSim,
                DebugCommand::DumpState,
                DebugCommand::DumpAi,
                DebugCommand::ScenarioSetup {
                    scenario_id: "combat_chaser".to_string(),
                },
                DebugCommand::FloorSet {
                    floor: FloorId::Basement,
                },
                DebugCommand::SwitchScene { scene: SceneKey::A },
                DebugCommand::Quit,
                DebugCommand::Despawn { entity_id: 42 },
                DebugCommand::Spawn {
                    def_name: "proto.worker".to_string(),
                    position: Some((1.5, -2.0)),
                },
                DebugCommand::Select { entity_id: 43 },
                DebugCommand::OrderMove { x: 4.0, y: -8.0 },
                DebugCommand::OrderInteract {
                    target_entity_id: 44
                },
                DebugCommand::InjectInput {
                    event: InjectedInputEvent::KeyDown {
                        key: InjectedKey::W,
                    },
                },
                DebugCommand::InjectInput {
                    event: InjectedInputEvent::KeyUp {
                        key: InjectedKey::Right,
                    },
                },
                DebugCommand::InjectInput {
                    event: InjectedInputEvent::MouseMove { x: 10.0, y: -4.0 },
                },
                DebugCommand::InjectInput {
                    event: InjectedInputEvent::MouseDown {
                        button: InjectedMouseButton::Left,
                    },
                },
                DebugCommand::InjectInput {
                    event: InjectedInputEvent::MouseUp {
                        button: InjectedMouseButton::Right,
                    },
                },
            ]
        );
        assert!(collect_output(&console).is_empty());
    }

    #[test]
    fn input_commands_validate_bad_args_with_usage() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("input.key_down");
        console.push_pending_line_for_test("input.key_up nope");
        console.push_pending_line_for_test("input.mouse_move x 1");
        console.push_pending_line_for_test("input.mouse_down middle");
        console.push_pending_line_for_test("input.mouse_up");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec![
                "error: expected exactly one argument <key>. usage: input.key_down <key>",
                "error: invalid key 'nope' (expected w|a|s|d|up|down|left|right|i|j|k|l). usage: input.key_up <key>",
                "error: invalid x coordinate 'x' (expected f32). usage: input.mouse_move <x> <y>",
                "error: invalid button 'middle' (expected left|right). usage: input.mouse_down <button>",
                "error: expected exactly one argument <button>. usage: input.mouse_up <button>",
            ]
        );
    }

    #[test]
    fn select_and_order_commands_validate_bad_args_with_usage() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("select");
        console.push_pending_line_for_test("select nope");
        console.push_pending_line_for_test("order.move 1");
        console.push_pending_line_for_test("order.move x 2");
        console.push_pending_line_for_test("order.interact");
        console.push_pending_line_for_test("order.interact nope");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec![
                "error: expected exactly one argument <entity_id>. usage: select <entity_id>",
                "error: invalid entity id 'nope' (expected u64). usage: select <entity_id>",
                "error: expected exactly two arguments <x> <y>. usage: order.move <x> <y>",
                "error: invalid x coordinate 'x' (expected f32). usage: order.move <x> <y>",
                "error: expected exactly one argument <target_entity_id>. usage: order.interact <target_entity_id>",
                "error: invalid target entity id 'nope' (expected u64). usage: order.interact <target_entity_id>",
            ]
        );
    }

    #[test]
    fn sim_step_commands_validate_bad_args_with_usage() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("pause_sim now");
        console.push_pending_line_for_test("resume_sim now");
        console.push_pending_line_for_test("tick");
        console.push_pending_line_for_test("tick 0");
        console.push_pending_line_for_test("tick nope");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec![
                "error: unexpected extra arguments. usage: pause_sim",
                "error: unexpected extra arguments. usage: resume_sim",
                "error: expected exactly one argument <steps>. usage: tick <steps>",
                "error: steps must be > 0. usage: tick <steps>",
                "error: invalid steps 'nope' (expected u32 > 0). usage: tick <steps>",
            ]
        );
    }

    #[test]
    fn dump_commands_validate_bad_args_with_usage() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("dump.state now");
        console.push_pending_line_for_test("dump.ai now");
        console.push_pending_line_for_test("scenario.setup");
        console.push_pending_line_for_test("scenario.setup combat chaser");
        console.push_pending_line_for_test("floor.set");
        console.push_pending_line_for_test("floor.set attic");
        console.push_pending_line_for_test("floor.set main extra");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec![
                "error: unexpected extra arguments. usage: dump.state",
                "error: unexpected extra arguments. usage: dump.ai",
                "error: expected exactly one argument <scenario_id>. usage: scenario.setup <scenario_id>",
                "error: expected exactly one argument <scenario_id>. usage: scenario.setup <scenario_id>",
                "error: expected exactly one argument <rooftop|main|basement>. usage: floor.set <rooftop|main|basement>",
                "error: invalid floor 'attic' (expected rooftop|main|basement). usage: floor.set <rooftop|main|basement>",
                "error: expected exactly one argument <rooftop|main|basement>. usage: floor.set <rooftop|main|basement>",
            ]
        );
    }

    #[test]
    fn sync_command_validates_bad_args_with_usage() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("sync now");
        console.push_pending_line_for_test("thruport.status now");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec![
                "error: unexpected extra arguments. usage: sync",
                "error: unexpected extra arguments. usage: thruport.status"
            ]
        );
    }

    #[test]
    fn thruport_telemetry_command_validates_bad_args_with_usage() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("thruport.telemetry");
        console.push_pending_line_for_test("thruport.telemetry maybe");
        console.push_pending_line_for_test("thruport.telemetry on extra");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec![
                "error: expected exactly one argument <on|off>. usage: thruport.telemetry <on|off>",
                "error: invalid value 'maybe' (expected on|off). usage: thruport.telemetry <on|off>",
                "error: expected exactly one argument <on|off>. usage: thruport.telemetry <on|off>",
            ]
        );
    }

    #[test]
    fn switch_scene_only_accepts_a_or_b() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("switch_scene c");

        processor.process_pending_lines(&mut console);

        assert_eq!(
            collect_output(&console),
            vec!["error: unknown scene id 'c' (expected a|b). usage: switch_scene <scene_id>"]
        );
    }

    #[test]
    fn spawn_parses_optional_coordinates() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("spawn proto.worker");

        processor.process_pending_lines(&mut console);
        let mut queued = Vec::new();
        processor.drain_pending_debug_commands_into(&mut queued);
        assert_eq!(
            queued,
            vec![DebugCommand::Spawn {
                def_name: "proto.worker".to_string(),
                position: None,
            }]
        );
    }

    #[test]
    fn tokenizer_handles_quotes_and_errors() {
        assert_eq!(
            tokenize_line("spawn \"proto worker\" 1 2").expect("tokens"),
            vec!["spawn", "proto worker", "1", "2"]
        );
        assert!(tokenize_line("echo \"oops").is_err());
    }

    #[test]
    fn processor_drains_lines_once() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        console.push_pending_line_for_test("reset_scene");

        processor.process_pending_lines(&mut console);
        processor.process_pending_lines(&mut console);

        let mut queued = Vec::new();
        processor.drain_pending_debug_commands_into(&mut queued);
        assert_eq!(queued, vec![DebugCommand::ResetScene]);
    }

    #[test]
    fn pending_queue_is_bounded() {
        let mut processor = ConsoleCommandProcessor::new();
        let mut console = ConsoleState::default();
        for _ in 0..(MAX_PENDING_DEBUG_COMMANDS + 4) {
            console.push_pending_line_for_test("quit");
        }

        processor.process_pending_lines(&mut console);
        let mut queued = Vec::new();
        processor.drain_pending_debug_commands_into(&mut queued);
        let expected = MAX_PENDING_DEBUG_COMMANDS.min(super::super::console::MAX_PENDING_LINES);
        assert_eq!(queued.len(), expected);
    }
}
