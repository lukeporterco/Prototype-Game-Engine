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

#[derive(Debug, Default)]
pub(crate) struct DevThruport {
    _private: (),
}

#[derive(Debug, Default)]
pub(crate) struct DevThruportHooks {
    _private: (),
}

struct NoOpConsoleInputQueueHook;
struct NoOpConsoleOutputTeeHook;
struct NoOpInputInjectionHook;

impl ConsoleInputQueueHook for NoOpConsoleInputQueueHook {
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
        Self { _private: () }
    }
}

pub(crate) fn initialize(_hooks: DevThruportHooks) -> DevThruport {
    exercise_hook_contracts_for_no_op_build();
    DevThruport { _private: () }
}

fn exercise_hook_contracts_for_no_op_build() {
    let mut queue_hook = NoOpConsoleInputQueueHook;
    let mut out = Vec::new();
    queue_hook.drain_pending_lines(&mut out);

    let mut tee_hook = NoOpConsoleOutputTeeHook;
    tee_hook.tee_output_line("");

    let mut input_hook = NoOpInputInjectionHook;
    input_hook.inject_input(InjectedInput::NoOp);
    input_hook.inject_input(InjectedInput::KeyDown);
    input_hook.inject_input(InjectedInput::KeyUp);
    input_hook.inject_input(InjectedInput::MouseMove { x: 0.0, y: 0.0 });
}

#[cfg(test)]
mod tests {
    use super::{initialize, DevThruportHooks};

    #[test]
    fn initialize_no_op_constructs_without_panic() {
        let hooks = DevThruportHooks::no_op();
        let _thruport = initialize(hooks);
    }
}
