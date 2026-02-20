use std::collections::{HashMap, VecDeque};

use crate::app::SceneKey;

use super::ConsoleState;

const MAX_PENDING_DEBUG_COMMANDS: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DebugCommand {
    ResetScene,
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
        assert_eq!(
            lines[4],
            "switch_scene <scene_id:a|b> - Switch active scene"
        );
        assert_eq!(lines[5], "quit - Quit app");
        assert_eq!(lines[6], "despawn <entity_id:u64> - Despawn entity by id");
        assert_eq!(
            lines[7],
            "spawn <def_name:string> [x:f32 y:f32] - Spawn entity by def name"
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
        console.push_pending_line_for_test("switch_scene a");
        console.push_pending_line_for_test("quit");
        console.push_pending_line_for_test("despawn 42");
        console.push_pending_line_for_test("spawn proto.worker 1.5 -2.0");

        processor.process_pending_lines(&mut console);

        let mut queued = Vec::new();
        processor.drain_pending_debug_commands_into(&mut queued);
        assert_eq!(
            queued,
            vec![
                DebugCommand::ResetScene,
                DebugCommand::SwitchScene { scene: SceneKey::A },
                DebugCommand::Quit,
                DebugCommand::Despawn { entity_id: 42 },
                DebugCommand::Spawn {
                    def_name: "proto.worker".to_string(),
                    position: Some((1.5, -2.0)),
                },
            ]
        );
        assert!(collect_output(&console).is_empty());
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
