use std::collections::VecDeque;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use super::overlay::draw_text_clipped_with_fallback;

const GLYPH_HEIGHT: i32 = 5;
const TEXT_SCALE: i32 = 3;
const LINE_ADVANCE: i32 = (GLYPH_HEIGHT + 2) * TEXT_SCALE;
const CONSOLE_PADDING: i32 = 6 * TEXT_SCALE;
const CONSOLE_BG_COLOR: [u8; 4] = [16, 16, 18, 255];
const CONSOLE_TEXT_COLOR: [u8; 4] = [210, 230, 210, 255];
const CONSOLE_PROMPT_PREFIX: &str = "> ";

pub(crate) const MAX_HISTORY_LINES: usize = 64;
pub(crate) const MAX_OUTPUT_LINES: usize = 256;
pub(crate) const MAX_PENDING_LINES: usize = 64;
pub(crate) const MAX_CURRENT_LINE_CHARS: usize = 256;

#[derive(Debug, Default)]
pub(crate) struct ConsoleState {
    is_open: bool,
    current_line: String,
    history: VecDeque<String>,
    history_cursor: Option<usize>,
    history_draft: Option<String>,
    output_lines: VecDeque<String>,
    pending_lines: VecDeque<String>,
}

impl ConsoleState {
    pub(crate) fn is_open(&self) -> bool {
        self.is_open
    }

    pub(crate) fn toggle_open(&mut self) {
        self.is_open = !self.is_open;
        self.clear_input_line_state();
    }

    pub(crate) fn handle_key_event(&mut self, key_event: &KeyEvent) {
        if !self.is_open || key_event.state != ElementState::Pressed {
            return;
        }

        let PhysicalKey::Code(code) = key_event.physical_key else {
            return;
        };
        self.handle_key_code(code);
    }

    pub(crate) fn handle_text_input_from_key_event(&mut self, key_event: &KeyEvent) {
        if !self.is_open || key_event.state != ElementState::Pressed {
            return;
        }

        let Some(text) = key_event.text.as_ref() else {
            return;
        };

        self.append_printable_text(text);
    }

    pub(crate) fn output_lines(&self) -> impl Iterator<Item = &str> {
        self.output_lines.iter().map(String::as_str)
    }

    pub(crate) fn current_line(&self) -> &str {
        &self.current_line
    }

    #[allow(dead_code)]
    pub(crate) fn drain_pending_lines_into(&mut self, out: &mut Vec<String>) {
        out.extend(self.pending_lines.drain(..));
    }

    fn handle_key_code(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Backspace => {
                self.current_line.pop();
            }
            KeyCode::Enter => {
                self.submit_current_line();
            }
            KeyCode::Escape => {
                self.is_open = false;
                self.clear_input_line_state();
            }
            KeyCode::ArrowUp => {
                self.navigate_history_up();
            }
            KeyCode::ArrowDown => {
                self.navigate_history_down();
            }
            _ => {}
        }
    }

    fn append_printable_text(&mut self, text: &str) {
        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            if self.current_line.chars().count() >= MAX_CURRENT_LINE_CHARS {
                break;
            }
            self.current_line.push(ch);
        }
    }

    fn clear_input_line_state(&mut self) {
        self.current_line.clear();
        self.history_cursor = None;
        self.history_draft = None;
    }

    fn submit_current_line(&mut self) {
        let raw_line = self.current_line.clone();
        push_bounded(&mut self.history, raw_line.clone(), MAX_HISTORY_LINES);
        push_bounded(
            &mut self.output_lines,
            format!("{CONSOLE_PROMPT_PREFIX}{raw_line}"),
            MAX_OUTPUT_LINES,
        );
        push_bounded(&mut self.pending_lines, raw_line, MAX_PENDING_LINES);
        self.clear_input_line_state();
    }

    fn navigate_history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        if self.history_cursor.is_none() {
            self.history_draft = Some(self.current_line.clone());
        }

        let next_index = match self.history_cursor {
            Some(index) => index.saturating_sub(1),
            None => self.history.len() - 1,
        };
        self.history_cursor = Some(next_index);
        self.current_line = self.history[next_index].clone();
    }

    fn navigate_history_down(&mut self) {
        let Some(index) = self.history_cursor else {
            return;
        };

        if index + 1 < self.history.len() {
            let next_index = index + 1;
            self.history_cursor = Some(next_index);
            self.current_line = self.history[next_index].clone();
            return;
        }

        self.history_cursor = None;
        self.current_line = self.history_draft.take().unwrap_or_default();
    }
}

fn push_bounded(queue: &mut VecDeque<String>, value: String, max_len: usize) {
    if queue.len() == max_len {
        queue.pop_front();
    }
    queue.push_back(value);
}

pub(crate) fn draw_console(frame: &mut [u8], width: u32, height: u32, state: &ConsoleState) {
    if !state.is_open() || width == 0 || height == 0 {
        return;
    }

    let output_lines: Vec<&str> = state.output_lines().collect();
    let max_output_lines = max_visible_output_lines(height);
    let output_line_count = output_lines.len().min(max_output_lines);
    let console_line_count = output_line_count as i32 + 1;
    let panel_height = console_line_count * LINE_ADVANCE + 2 * CONSOLE_PADDING;

    let top = (height as i32 - panel_height).max(0);
    draw_filled_rect(
        frame,
        width,
        height,
        0,
        top,
        width as i32,
        panel_height,
        CONSOLE_BG_COLOR,
    );

    let prompt = format!("{CONSOLE_PROMPT_PREFIX}{}", state.current_line());
    let prompt_y = height as i32 - CONSOLE_PADDING - LINE_ADVANCE;
    draw_text_clipped_with_fallback(
        frame,
        width,
        height,
        CONSOLE_PADDING,
        prompt_y,
        &prompt,
        CONSOLE_TEXT_COLOR,
        '?',
    );

    let first_output_index = output_lines.len().saturating_sub(output_line_count);
    let mut line_y = prompt_y - LINE_ADVANCE;
    for line in output_lines.iter().skip(first_output_index).rev() {
        draw_text_clipped_with_fallback(
            frame,
            width,
            height,
            CONSOLE_PADDING,
            line_y,
            line,
            CONSOLE_TEXT_COLOR,
            '?',
        );
        line_y -= LINE_ADVANCE;
    }
}

fn max_visible_output_lines(height: u32) -> usize {
    let usable = height as i32 - 2 * CONSOLE_PADDING - LINE_ADVANCE;
    if usable <= 0 {
        0
    } else {
        (usable / LINE_ADVANCE) as usize
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_filled_rect(
    frame: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    rect_width: i32,
    rect_height: i32,
    color: [u8; 4],
) {
    let start_x = x.max(0);
    let start_y = y.max(0);
    let end_x = (x + rect_width).min(width as i32);
    let end_y = (y + rect_height).min(height as i32);
    if end_x <= start_x || end_y <= start_y {
        return;
    }

    let width_usize = width as usize;
    for py in start_y..end_y {
        for px in start_x..end_x {
            let pixel = py as usize * width_usize + px as usize;
            let byte = pixel * 4;
            if byte + 3 >= frame.len() {
                continue;
            }
            frame[byte..byte + 4].copy_from_slice(&color);
        }
    }
}

#[cfg(test)]
mod tests {
    use winit::keyboard::KeyCode;

    use super::*;

    #[test]
    fn toggle_open_close_works() {
        let mut console = ConsoleState::default();
        assert!(!console.is_open());
        console.toggle_open();
        assert!(console.is_open());
        console.toggle_open();
        assert!(!console.is_open());
    }

    #[test]
    fn escape_closes_and_clears_input_line() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        console.current_line = "abc".to_string();
        console.history_cursor = Some(0);
        console.history_draft = Some("draft".to_string());

        console.handle_key_code(KeyCode::Escape);

        assert!(!console.is_open());
        assert_eq!(console.current_line(), "");
        assert!(console.history_cursor.is_none());
        assert!(console.history_draft.is_none());
    }

    #[test]
    fn text_input_accepts_printable_and_ignores_control() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        console.append_printable_text("a\n");
        assert_eq!(console.current_line(), "a");
    }

    #[test]
    fn backspace_removes_character_safely() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        console.current_line = "ab".to_string();
        console.handle_key_code(KeyCode::Backspace);
        console.handle_key_code(KeyCode::Backspace);
        console.handle_key_code(KeyCode::Backspace);
        assert_eq!(console.current_line(), "");
    }

    #[test]
    fn enter_submits_echoes_and_enqueues_raw_line() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        console.current_line = "test".to_string();
        console.handle_key_code(KeyCode::Enter);

        assert_eq!(console.current_line(), "");
        assert_eq!(console.history.back().map(String::as_str), Some("test"));
        assert_eq!(
            console.output_lines.back().map(String::as_str),
            Some("> test")
        );
        assert_eq!(
            console.pending_lines.back().map(String::as_str),
            Some("test")
        );
    }

    #[test]
    fn history_up_down_cycles_and_restores_draft() {
        let mut console = ConsoleState::default();
        console.toggle_open();

        for line in ["alpha", "beta"] {
            console.current_line = line.to_string();
            console.handle_key_code(KeyCode::Enter);
        }

        console.current_line = "draft".to_string();
        console.handle_key_code(KeyCode::ArrowUp);
        assert_eq!(console.current_line(), "beta");

        console.handle_key_code(KeyCode::ArrowUp);
        assert_eq!(console.current_line(), "alpha");

        console.handle_key_code(KeyCode::ArrowDown);
        assert_eq!(console.current_line(), "beta");

        console.handle_key_code(KeyCode::ArrowDown);
        assert_eq!(console.current_line(), "draft");
    }

    #[test]
    fn bounded_buffers_drop_oldest_entries() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        for idx in 0..(MAX_HISTORY_LINES + 2) {
            console.current_line = format!("h{idx}");
            console.handle_key_code(KeyCode::Enter);
        }
        assert_eq!(console.history.len(), MAX_HISTORY_LINES);
        assert_eq!(console.history.front().map(String::as_str), Some("h2"));

        for idx in 0..(MAX_OUTPUT_LINES + 2) {
            push_bounded(
                &mut console.output_lines,
                format!("o{idx}"),
                MAX_OUTPUT_LINES,
            );
        }
        assert_eq!(console.output_lines.len(), MAX_OUTPUT_LINES);
        assert_eq!(console.output_lines.front().map(String::as_str), Some("o2"));

        for idx in 0..(MAX_PENDING_LINES + 2) {
            push_bounded(
                &mut console.pending_lines,
                format!("p{idx}"),
                MAX_PENDING_LINES,
            );
        }
        assert_eq!(console.pending_lines.len(), MAX_PENDING_LINES);
        assert_eq!(
            console.pending_lines.front().map(String::as_str),
            Some("p2")
        );
    }

    #[test]
    fn current_line_has_character_cap() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        let over_limit = "x".repeat(MAX_CURRENT_LINE_CHARS + 20);
        console.append_printable_text(&over_limit);
        assert_eq!(console.current_line.chars().count(), MAX_CURRENT_LINE_CHARS);
    }

    #[test]
    fn pending_lines_drain_api_empties_queue() {
        let mut console = ConsoleState::default();
        console.toggle_open();
        console.current_line = "one".to_string();
        console.handle_key_code(KeyCode::Enter);
        console.current_line = "two".to_string();
        console.handle_key_code(KeyCode::Enter);

        let mut drained = Vec::new();
        console.drain_pending_lines_into(&mut drained);
        assert_eq!(drained, vec!["one".to_string(), "two".to_string()]);
        assert!(console.pending_lines.is_empty());
    }

    #[test]
    fn draw_console_is_safe_on_tiny_viewports() {
        let mut frame = vec![0u8; 4];
        let mut console = ConsoleState::default();
        console.toggle_open();
        console.current_line = "abc".to_string();
        draw_console(&mut frame, 1, 1, &console);
        assert_eq!(frame.len(), 4);
    }
}
