use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::app::Vec2;

use super::overlay::draw_text_clipped_with_fallback;

const GLYPH_HEIGHT: i32 = 5;
const TEXT_SCALE: i32 = 3;
const LINE_ADVANCE: i32 = (GLYPH_HEIGHT + 2) * TEXT_SCALE;
const PANEL_PADDING: i32 = 5 * TEXT_SCALE;
const PANEL_GAP_PX: i32 = 3 * TEXT_SCALE;
const PANEL_WIDTH_PX: i32 = 360;
const BUTTON_HEIGHT_PX: i32 = 18;
const BUTTON_COL_GAP_PX: i32 = 8;
const BUTTON_ROW_GAP_PX: i32 = 6;
const PANEL_MARGIN_PX: i32 = 12;
const PANEL_BG_COLOR: [u8; 4] = [10, 12, 16, 235];
const PANEL_BORDER_COLOR: [u8; 4] = [72, 82, 96, 255];
const HEADER_COLOR: [u8; 4] = [235, 230, 170, 255];
const BUTTON_TEXT_COLOR: [u8; 4] = [215, 225, 235, 255];
const BUTTON_BG_COLOR: [u8; 4] = [34, 40, 52, 255];
const BUTTON_BORDER_COLOR: [u8; 4] = [92, 106, 130, 255];
const BUTTON_ARMED_BG_COLOR: [u8; 4] = [66, 84, 45, 255];
const TOOLTIP_BG_COLOR: [u8; 4] = [16, 18, 24, 255];
const TOOLTIP_BORDER_COLOR: [u8; 4] = [198, 218, 145, 255];
const TOOLTIP_TEXT_COLOR: [u8; 4] = [240, 245, 205, 255];
const STATUS_TEXT_COLOR: [u8; 4] = [255, 168, 120, 255];
const MAX_MACROS: usize = 24;
const MAX_COMMANDS_PER_MACRO: usize = 16;
const MAX_QUEUED_BYTES_PER_MACRO_CLICK: usize = 4096;
const MACROS_FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RectPx {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl RectPx {
    fn right(self) -> i32 {
        self.left + self.width
    }

    fn bottom(self) -> i32 {
        self.top + self.height
    }

    fn contains(self, point: Vec2) -> bool {
        let x = point.x.round() as i32;
        let y = point.y.round() as i32;
        x >= self.left && x < self.right() && y >= self.top && y < self.bottom()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandPaletteButtonKind {
    Immediate { command: String },
    SpawnPlacement { def_name: String },
    Macro { macro_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandPaletteButton {
    pub label: String,
    pub kind: CommandPaletteButtonKind,
    pub rect: RectPx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandPaletteHeader {
    pub text: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommandPaletteRenderData {
    pub panel_rect: RectPx,
    pub headers: Vec<CommandPaletteHeader>,
    pub buttons: Vec<CommandPaletteButton>,
    pub armed_tooltip: Option<String>,
    pub armed_tooltip_origin: Option<Vec2>,
    pub status_line: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandPaletteState {
    enabled: bool,
    armed_spawn_def: Option<String>,
    status_line: Option<String>,
    render_data: Option<CommandPaletteRenderData>,
    macro_file_path: Option<PathBuf>,
    macros: Vec<PaletteMacro>,
    macros_load_state: PaletteMacrosLoadState,
    macros_load_error_pending: Option<String>,
}

impl CommandPaletteState {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            armed_spawn_def: None,
            status_line: None,
            render_data: None,
            macro_file_path: None,
            macros: Vec::new(),
            macros_load_state: PaletteMacrosLoadState::NotLoaded,
            macros_load_error_pending: None,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn is_armed(&self) -> bool {
        self.armed_spawn_def.is_some()
    }

    pub(crate) fn arm_spawn(&mut self, def_name: impl Into<String>) {
        self.armed_spawn_def = Some(def_name.into());
        self.status_line = None;
    }

    pub(crate) fn take_armed_spawn(&mut self) -> Option<String> {
        self.armed_spawn_def.take()
    }

    pub(crate) fn clear_armed_spawn(&mut self) {
        self.armed_spawn_def = None;
    }

    pub(crate) fn set_status_line(&mut self, status: impl Into<String>) {
        self.status_line = Some(status.into());
    }

    pub(crate) fn clear_status_line(&mut self) {
        self.status_line = None;
    }

    pub(crate) fn set_macro_file_path(&mut self, path: PathBuf) {
        self.macro_file_path = Some(path);
        self.macros.clear();
        self.macros_load_state = PaletteMacrosLoadState::NotLoaded;
        self.macros_load_error_pending = None;
    }

    pub(crate) fn take_load_error_line(&mut self) -> Option<String> {
        self.macros_load_error_pending.take()
    }

    pub(crate) fn macro_by_index(&self, macro_index: usize) -> Option<&PaletteMacro> {
        self.macros.get(macro_index)
    }

    pub(crate) fn is_macro_within_queue_limit(macro_def: &PaletteMacro) -> bool {
        macro_total_queued_bytes(macro_def) <= MAX_QUEUED_BYTES_PER_MACRO_CLICK
    }

    pub(crate) fn rebuild_layout(
        &mut self,
        viewport: (u32, u32),
        spawn_def_names: &[String],
        cursor_position_px: Option<Vec2>,
    ) {
        if !self.enabled {
            self.render_data = None;
            return;
        }
        self.load_macros_if_needed();

        self.render_data = Some(build_render_data(
            viewport,
            spawn_def_names,
            &self.macros,
            self.armed_spawn_def.as_deref(),
            self.status_line.as_deref(),
            cursor_position_px,
        ));
    }

    pub(crate) fn render_data(&self) -> Option<&CommandPaletteRenderData> {
        self.render_data.as_ref()
    }

    pub(crate) fn button_at_cursor(
        &self,
        cursor_position_px: Vec2,
    ) -> Option<CommandPaletteButtonKind> {
        let data = self.render_data.as_ref()?;
        data.buttons
            .iter()
            .find(|button| button.rect.contains(cursor_position_px))
            .map(|button| button.kind.clone())
    }

    pub(crate) fn is_cursor_over_panel(&self, cursor_position_px: Vec2) -> bool {
        self.render_data
            .as_ref()
            .is_some_and(|data| data.panel_rect.contains(cursor_position_px))
    }

    fn load_macros_if_needed(&mut self) {
        if self.macros_load_state != PaletteMacrosLoadState::NotLoaded {
            return;
        }
        self.macros_load_state = PaletteMacrosLoadState::Loaded;
        self.macros.clear();
        self.macros_load_error_pending = None;
        let Some(path) = self.macro_file_path.as_deref() else {
            return;
        };
        if !path.is_file() {
            return;
        }

        match load_palette_macros_file(path) {
            Ok(macros) => {
                self.macros = macros;
            }
            Err(error) => {
                self.macros.clear();
                self.macros_load_state = PaletteMacrosLoadState::Failed;
                self.macros_load_error_pending =
                    Some(format!("command palette macros load failed: {error}"));
            }
        }
    }
}

pub(crate) fn format_spawn_command(def_name: &str, world: Vec2) -> String {
    format!("spawn {def_name} {:.2} {:.2}", world.x, world.y)
}

pub(crate) fn draw_command_palette(
    frame: &mut [u8],
    width: u32,
    height: u32,
    data: &CommandPaletteRenderData,
) {
    if width == 0 || height == 0 {
        return;
    }

    draw_filled_rect(frame, width, height, data.panel_rect, PANEL_BG_COLOR);
    draw_rect_outline(frame, width, height, data.panel_rect, PANEL_BORDER_COLOR);

    for header in &data.headers {
        draw_text_clipped_with_fallback(
            frame,
            width,
            height,
            header.x,
            header.y,
            &header.text,
            HEADER_COLOR,
            '?',
        );
    }

    for button in &data.buttons {
        let button_color = match &button.kind {
            CommandPaletteButtonKind::SpawnPlacement { def_name } => {
                if data
                    .armed_tooltip
                    .as_deref()
                    .is_some_and(|text| text.ends_with(def_name))
                {
                    BUTTON_ARMED_BG_COLOR
                } else {
                    BUTTON_BG_COLOR
                }
            }
            CommandPaletteButtonKind::Immediate { .. } | CommandPaletteButtonKind::Macro { .. } => {
                BUTTON_BG_COLOR
            }
        };
        draw_filled_rect(frame, width, height, button.rect, button_color);
        draw_rect_outline(frame, width, height, button.rect, BUTTON_BORDER_COLOR);
        draw_text_clipped_with_fallback(
            frame,
            width,
            height,
            button.rect.left + 6,
            button.rect.top + 3,
            &button.label,
            BUTTON_TEXT_COLOR,
            '?',
        );
    }

    if let Some(status_line) = data.status_line.as_ref() {
        let status_y = data.panel_rect.bottom() - PANEL_PADDING - LINE_ADVANCE;
        draw_text_clipped_with_fallback(
            frame,
            width,
            height,
            data.panel_rect.left + PANEL_PADDING,
            status_y,
            status_line,
            STATUS_TEXT_COLOR,
            '?',
        );
    }

    if let (Some(tooltip), Some(origin)) = (
        data.armed_tooltip.as_ref(),
        data.armed_tooltip_origin.as_ref().copied(),
    ) {
        let tooltip_rect = RectPx {
            left: origin.x.round() as i32 + 14,
            top: origin.y.round() as i32 + 14,
            width: 260,
            height: LINE_ADVANCE + 8,
        };
        draw_filled_rect(frame, width, height, tooltip_rect, TOOLTIP_BG_COLOR);
        draw_rect_outline(frame, width, height, tooltip_rect, TOOLTIP_BORDER_COLOR);
        draw_text_clipped_with_fallback(
            frame,
            width,
            height,
            tooltip_rect.left + 6,
            tooltip_rect.top + 4,
            tooltip,
            TOOLTIP_TEXT_COLOR,
            '?',
        );
    }
}

fn build_render_data(
    viewport: (u32, u32),
    spawn_def_names: &[String],
    macros: &[PaletteMacro],
    armed_spawn_def: Option<&str>,
    status_line: Option<&str>,
    cursor_position_px: Option<Vec2>,
) -> CommandPaletteRenderData {
    let mut y_cursor = PANEL_MARGIN_PX + PANEL_PADDING;
    let mut headers = Vec::<CommandPaletteHeader>::new();
    let panel_width = PANEL_WIDTH_PX
        .min(viewport.0 as i32 - PANEL_MARGIN_PX * 2)
        .max(220);
    let panel_x = viewport.0 as i32 - PANEL_MARGIN_PX - panel_width;
    let content_left = panel_x + PANEL_PADDING;
    let content_width = panel_width - PANEL_PADDING * 2;
    let button_width = ((content_width - BUTTON_COL_GAP_PX) / 2).max(80);
    let mut buttons = Vec::<CommandPaletteButton>::new();

    y_cursor = push_section_header(&mut headers, content_left, y_cursor, "Command Palette");
    y_cursor = push_section_header(&mut headers, content_left, y_cursor, "Sim");
    let sim_commands = [
        "pause_sim",
        "resume_sim",
        "tick 1",
        "sync",
        "reset_scene",
        "quit",
    ];
    y_cursor = push_immediate_buttons(
        &mut buttons,
        content_left,
        button_width,
        y_cursor,
        &sim_commands,
    );

    y_cursor = push_section_header(&mut headers, content_left, y_cursor, "Scenario");
    let scenario_commands = [
        "scenario.setup combat_chaser",
        "scenario.setup visual_sandbox",
    ];
    y_cursor = push_immediate_buttons(
        &mut buttons,
        content_left,
        button_width,
        y_cursor,
        &scenario_commands,
    );

    if !macros.is_empty() {
        y_cursor = push_section_header(&mut headers, content_left, y_cursor, "Macros");
        y_cursor = push_macro_buttons(&mut buttons, content_left, button_width, y_cursor, macros);
    }

    y_cursor = push_section_header(&mut headers, content_left, y_cursor, "Spawn (placement)");
    let spawn_labels: Vec<String> = spawn_def_names
        .iter()
        .map(|def_name| format!("spawn {def_name}"))
        .collect();
    let row_count = spawn_labels.len().div_ceil(2);
    for row in 0..row_count {
        let top = y_cursor + row as i32 * (BUTTON_HEIGHT_PX + BUTTON_ROW_GAP_PX);
        for col in 0..2 {
            let index = row * 2 + col;
            if index >= spawn_labels.len() {
                continue;
            }
            let def_name = spawn_def_names[index].clone();
            let left = content_left + col as i32 * (button_width + BUTTON_COL_GAP_PX);
            buttons.push(CommandPaletteButton {
                label: spawn_labels[index].clone(),
                kind: CommandPaletteButtonKind::SpawnPlacement { def_name },
                rect: RectPx {
                    left,
                    top,
                    width: button_width,
                    height: BUTTON_HEIGHT_PX,
                },
            });
        }
    }
    y_cursor += row_count as i32 * (BUTTON_HEIGHT_PX + BUTTON_ROW_GAP_PX);

    let status_lines = if status_line.is_some() { 1 } else { 0 };
    let panel_height = (y_cursor + PANEL_PADDING + status_lines * LINE_ADVANCE + PANEL_GAP_PX)
        .max(PANEL_PADDING * 2 + LINE_ADVANCE * 4);
    let panel_rect = RectPx {
        left: panel_x,
        top: PANEL_MARGIN_PX,
        width: panel_width,
        height: panel_height,
    };

    let armed_tooltip = armed_spawn_def.map(|def_name| format!("spawn {def_name}"));
    CommandPaletteRenderData {
        panel_rect,
        headers,
        buttons,
        armed_tooltip,
        armed_tooltip_origin: cursor_position_px,
        status_line: status_line.map(ToString::to_string),
    }
}

fn push_section_header(
    out: &mut Vec<CommandPaletteHeader>,
    x: i32,
    y_cursor: i32,
    header: &str,
) -> i32 {
    out.push(CommandPaletteHeader {
        text: header.to_string(),
        x,
        y: y_cursor,
    });
    y_cursor + LINE_ADVANCE + PANEL_GAP_PX
}

fn push_immediate_buttons(
    out: &mut Vec<CommandPaletteButton>,
    left: i32,
    button_width: i32,
    y_cursor: i32,
    commands: &[&str],
) -> i32 {
    let row_count = commands.len().div_ceil(2);
    for row in 0..row_count {
        let top = y_cursor + row as i32 * (BUTTON_HEIGHT_PX + BUTTON_ROW_GAP_PX);
        for col in 0..2 {
            let index = row * 2 + col;
            if index >= commands.len() {
                continue;
            }
            let command = commands[index].to_string();
            let label = command.clone();
            let x = left + col as i32 * (button_width + BUTTON_COL_GAP_PX);
            out.push(CommandPaletteButton {
                label,
                kind: CommandPaletteButtonKind::Immediate { command },
                rect: RectPx {
                    left: x,
                    top,
                    width: button_width,
                    height: BUTTON_HEIGHT_PX,
                },
            });
        }
    }
    y_cursor + row_count as i32 * (BUTTON_HEIGHT_PX + BUTTON_ROW_GAP_PX)
}

fn push_macro_buttons(
    out: &mut Vec<CommandPaletteButton>,
    left: i32,
    button_width: i32,
    y_cursor: i32,
    macros: &[PaletteMacro],
) -> i32 {
    let row_count = macros.len().div_ceil(2);
    for row in 0..row_count {
        let top = y_cursor + row as i32 * (BUTTON_HEIGHT_PX + BUTTON_ROW_GAP_PX);
        for col in 0..2 {
            let index = row * 2 + col;
            if index >= macros.len() {
                continue;
            }
            let x = left + col as i32 * (button_width + BUTTON_COL_GAP_PX);
            out.push(CommandPaletteButton {
                label: macros[index].name.clone(),
                kind: CommandPaletteButtonKind::Macro { macro_index: index },
                rect: RectPx {
                    left: x,
                    top,
                    width: button_width,
                    height: BUTTON_HEIGHT_PX,
                },
            });
        }
    }
    y_cursor + row_count as i32 * (BUTTON_HEIGHT_PX + BUTTON_ROW_GAP_PX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaletteMacro {
    pub name: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteMacrosLoadState {
    NotLoaded,
    Loaded,
    Failed,
}

#[derive(Debug, Clone, Deserialize)]
struct PaletteMacrosFile {
    version: u32,
    #[serde(default)]
    macros: Vec<PaletteMacroDef>,
}

#[derive(Debug, Clone, Deserialize)]
struct PaletteMacroDef {
    name: String,
    #[serde(default)]
    commands: Vec<String>,
}

fn load_palette_macros_file(path: &Path) -> Result<Vec<PaletteMacro>, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let file: PaletteMacrosFile = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    if file.version != MACROS_FILE_VERSION {
        return Err(format!(
            "unsupported version {}; expected {}",
            file.version, MACROS_FILE_VERSION
        ));
    }
    if file.macros.len() > MAX_MACROS {
        return Err(format!(
            "macro count {} exceeds limit {}",
            file.macros.len(),
            MAX_MACROS
        ));
    }
    file.macros
        .into_iter()
        .enumerate()
        .map(|(index, macro_def)| validate_palette_macro(index, macro_def))
        .collect()
}

fn validate_palette_macro(
    index: usize,
    macro_def: PaletteMacroDef,
) -> Result<PaletteMacro, String> {
    let name = macro_def.name.trim().to_string();
    if name.is_empty() {
        return Err(format!("macro[{index}] has empty name"));
    }
    if macro_def.commands.len() > MAX_COMMANDS_PER_MACRO {
        return Err(format!(
            "macro[{index}] command count {} exceeds limit {}",
            macro_def.commands.len(),
            MAX_COMMANDS_PER_MACRO
        ));
    }
    let mut commands = Vec::with_capacity(macro_def.commands.len());
    for (command_index, command) in macro_def.commands.into_iter().enumerate() {
        if command.is_empty() {
            return Err(format!("macro[{index}] command[{command_index}] is empty"));
        }
        commands.push(command);
    }
    Ok(PaletteMacro { name, commands })
}

fn macro_total_queued_bytes(macro_def: &PaletteMacro) -> usize {
    macro_def.commands.iter().map(|line| line.len()).sum()
}

fn draw_filled_rect(frame: &mut [u8], width: u32, height: u32, rect: RectPx, color: [u8; 4]) {
    let start_x = rect.left.max(0);
    let start_y = rect.top.max(0);
    let end_x = rect.right().min(width as i32);
    let end_y = rect.bottom().min(height as i32);
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

fn draw_rect_outline(frame: &mut [u8], width: u32, height: u32, rect: RectPx, color: [u8; 4]) {
    if rect.width <= 1 || rect.height <= 1 {
        return;
    }
    draw_hline(
        frame,
        width,
        height,
        rect.left,
        rect.right() - 1,
        rect.top,
        color,
    );
    draw_hline(
        frame,
        width,
        height,
        rect.left,
        rect.right() - 1,
        rect.bottom() - 1,
        color,
    );
    draw_vline(
        frame,
        width,
        height,
        rect.left,
        rect.top,
        rect.bottom() - 1,
        color,
    );
    draw_vline(
        frame,
        width,
        height,
        rect.right() - 1,
        rect.top,
        rect.bottom() - 1,
        color,
    );
}

fn draw_hline(frame: &mut [u8], width: u32, height: u32, x0: i32, x1: i32, y: i32, color: [u8; 4]) {
    if y < 0 || y >= height as i32 {
        return;
    }
    let start = x0.max(0);
    let end = x1.min(width as i32 - 1);
    if end < start {
        return;
    }
    for x in start..=end {
        write_pixel(frame, width as usize, x as usize, y as usize, color);
    }
}

fn draw_vline(frame: &mut [u8], width: u32, height: u32, x: i32, y0: i32, y1: i32, color: [u8; 4]) {
    if x < 0 || x >= width as i32 {
        return;
    }
    let start = y0.max(0);
    let end = y1.min(height as i32 - 1);
    if end < start {
        return;
    }
    for y in start..=end {
        write_pixel(frame, width as usize, x as usize, y as usize, color);
    }
}

fn write_pixel(frame: &mut [u8], width: usize, x: usize, y: usize, color: [u8; 4]) {
    let Some(pixel_offset) = y.checked_mul(width).and_then(|row| row.checked_add(x)) else {
        return;
    };
    let Some(byte_offset) = pixel_offset.checked_mul(4) else {
        return;
    };
    let Some(end) = byte_offset.checked_add(4) else {
        return;
    };
    if end > frame.len() {
        return;
    }
    frame[byte_offset..end].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn format_spawn_command_uses_two_decimal_precision() {
        let command = format_spawn_command(
            "proto.player",
            Vec2 {
                x: 1.234,
                y: -5.678,
            },
        );
        assert_eq!(command, "spawn proto.player 1.23 -5.68");
    }

    #[test]
    fn hit_testing_identifies_buttons_and_panel_surface() {
        let mut state = CommandPaletteState::new(true);
        let defs = vec!["proto.player".to_string(), "proto.npc_dummy".to_string()];
        state.rebuild_layout((1280, 720), &defs, Some(Vec2 { x: 100.0, y: 100.0 }));
        let data = state.render_data().expect("render data");
        let button = data.buttons.first().expect("button");
        let cursor = Vec2 {
            x: (button.rect.left + 2) as f32,
            y: (button.rect.top + 2) as f32,
        };
        assert!(state.button_at_cursor(cursor).is_some());
        assert!(state.is_cursor_over_panel(cursor));
    }

    #[test]
    fn hit_testing_panel_excludes_outside_cursor() {
        let mut state = CommandPaletteState::new(true);
        let defs = vec!["proto.player".to_string()];
        state.rebuild_layout((1280, 720), &defs, None);
        assert!(!state.is_cursor_over_panel(Vec2 { x: 12.0, y: 12.0 }));
        assert!(state.button_at_cursor(Vec2 { x: 12.0, y: 12.0 }).is_none());
    }

    fn write_macros_file(root: &Path, content: &str) -> PathBuf {
        let tools_dir = root.join("tools");
        std::fs::create_dir_all(&tools_dir).expect("tools dir");
        let path = tools_dir.join("command_palette_macros.v1.json");
        std::fs::write(&path, content).expect("write macros file");
        path
    }

    #[test]
    fn macros_parse_and_load_success() {
        let temp = TempDir::new().expect("temp");
        let path = write_macros_file(
            temp.path(),
            r#"{"version":1,"macros":[{"name":"Setup","commands":["pause_sim","sync"]}]}"#,
        );
        let mut state = CommandPaletteState::new(true);
        state.set_macro_file_path(path);
        state.rebuild_layout((1280, 720), &[], None);
        assert!(state.take_load_error_line().is_none());
        let macro_def = state.macro_by_index(0).expect("macro");
        assert_eq!(macro_def.name, "Setup");
        assert_eq!(
            macro_def.commands,
            vec!["pause_sim".to_string(), "sync".to_string()]
        );
    }

    #[test]
    fn missing_macros_file_loads_empty_without_error() {
        let temp = TempDir::new().expect("temp");
        let mut state = CommandPaletteState::new(true);
        state.set_macro_file_path(
            temp.path()
                .join("tools")
                .join("command_palette_macros.v1.json"),
        );
        state.rebuild_layout((1280, 720), &[], None);
        assert!(state.take_load_error_line().is_none());
        assert!(state.macro_by_index(0).is_none());
    }

    #[test]
    fn malformed_macros_file_sets_one_shot_load_error() {
        let temp = TempDir::new().expect("temp");
        let path = write_macros_file(temp.path(), r#"{"version":1,"macros":[{"name":"x"}"#);
        let mut state = CommandPaletteState::new(true);
        state.set_macro_file_path(path);
        state.rebuild_layout((1280, 720), &[], None);
        let first = state.take_load_error_line().expect("error once");
        assert!(first.starts_with("command palette macros load failed:"));
        assert!(state.take_load_error_line().is_none());
    }

    #[test]
    fn unsupported_version_sets_one_shot_load_error() {
        let temp = TempDir::new().expect("temp");
        let path = write_macros_file(temp.path(), r#"{"version":2,"macros":[]}"#);
        let mut state = CommandPaletteState::new(true);
        state.set_macro_file_path(path);
        state.rebuild_layout((1280, 720), &[], None);
        let first = state.take_load_error_line().expect("error once");
        assert!(first.contains("unsupported version"));
        assert!(state.take_load_error_line().is_none());
    }

    #[test]
    fn macro_count_cap_rejects_file() {
        let temp = TempDir::new().expect("temp");
        let mut macro_entries = Vec::new();
        for idx in 0..=MAX_MACROS {
            macro_entries.push(format!(r#"{{"name":"m{idx}","commands":["sync"]}}"#));
        }
        let content = format!(r#"{{"version":1,"macros":[{}]}}"#, macro_entries.join(","));
        let path = write_macros_file(temp.path(), &content);
        let mut state = CommandPaletteState::new(true);
        state.set_macro_file_path(path);
        state.rebuild_layout((1280, 720), &[], None);
        let error = state.take_load_error_line().expect("error");
        assert!(error.contains("macro count"));
        assert!(state.macro_by_index(0).is_none());
    }

    #[test]
    fn commands_per_macro_cap_rejects_file() {
        let temp = TempDir::new().expect("temp");
        let mut commands = Vec::new();
        for _ in 0..=MAX_COMMANDS_PER_MACRO {
            commands.push(r#""sync""#.to_string());
        }
        let content = format!(
            r#"{{"version":1,"macros":[{{"name":"too_many","commands":[{}]}}]}}"#,
            commands.join(",")
        );
        let path = write_macros_file(temp.path(), &content);
        let mut state = CommandPaletteState::new(true);
        state.set_macro_file_path(path);
        state.rebuild_layout((1280, 720), &[], None);
        let error = state.take_load_error_line().expect("error");
        assert!(error.contains("command count"));
        assert!(state.macro_by_index(0).is_none());
    }

    #[test]
    fn macro_click_queue_bytes_cap_blocks_enqueue() {
        let mut commands = Vec::new();
        commands.push("a".repeat(MAX_QUEUED_BYTES_PER_MACRO_CLICK));
        commands.push("b".to_string());
        let macro_def = PaletteMacro {
            name: "big".to_string(),
            commands,
        };
        assert!(!CommandPaletteState::is_macro_within_queue_limit(
            &macro_def
        ));
    }
}
