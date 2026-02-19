#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Quit,
}

const ACTION_COUNT: usize = 5;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ActionStates {
    down: [bool; ACTION_COUNT],
}

impl ActionStates {
    pub(crate) fn set(&mut self, action: InputAction, is_down: bool) {
        self.down[action.index()] = is_down;
    }

    pub(crate) fn is_down(&self, action: InputAction) -> bool {
        self.down[action.index()]
    }
}

impl InputAction {
    const fn index(self) -> usize {
        match self {
            InputAction::MoveUp => 0,
            InputAction::MoveDown => 1,
            InputAction::MoveLeft => 2,
            InputAction::MoveRight => 3,
            InputAction::Quit => 4,
        }
    }
}
