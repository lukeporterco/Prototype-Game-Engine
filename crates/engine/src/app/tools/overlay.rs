use crate::app::{DebugInfoSnapshot, DebugJobState, EntityId, LoopMetricsSnapshot};

use super::PerfStatsSnapshot;

const GLYPH_WIDTH: i32 = 3;
const GLYPH_HEIGHT: i32 = 5;
const TEXT_SCALE: i32 = 3;
const GLYPH_ADVANCE: i32 = (GLYPH_WIDTH + 1) * TEXT_SCALE;
const LINE_ADVANCE: i32 = (GLYPH_HEIGHT + 2) * TEXT_SCALE;
const OVERLAY_PADDING: i32 = 6 * TEXT_SCALE;
const OVERLAY_PANEL_INSET_X: i32 = 4 * TEXT_SCALE;
const OVERLAY_PANEL_INSET_Y: i32 = 3 * TEXT_SCALE;
const OVERLAY_TEXT_PRIMARY_COLOR: [u8; 4] = [244, 248, 252, 255];
const OVERLAY_TEXT_DIM_COLOR: [u8; 4] = [176, 198, 220, 255];
const OVERLAY_PANEL_BG_COLOR: [u8; 4] = [10, 12, 16, 210];
const OVERLAY_PANEL_BORDER_COLOR: [u8; 4] = [92, 106, 126, 255];
const PERF_SECTION_LABEL: &str = "Perf";
const SCENE_SECTION_LABEL: &str = "Scene";
const INSPECT_SECTION_LABEL: &str = "Inspect";

#[derive(Debug, Clone)]
pub(crate) struct OverlayData {
    pub metrics: LoopMetricsSnapshot,
    pub perf: PerfStatsSnapshot,
    pub render_fps_cap: Option<u32>,
    pub slow_frame_delay_ms: u64,
    pub entity_count: usize,
    pub content_status: &'static str,
    pub selected_entity: Option<EntityId>,
    pub selected_target: Option<crate::app::Vec2>,
    pub resource_count: Option<u32>,
    pub debug_info: Option<DebugInfoSnapshot>,
}

pub(crate) fn draw_overlay(frame: &mut [u8], width: u32, height: u32, data: &OverlayData) {
    if width == 0 || height == 0 {
        return;
    }

    let lines = build_overlay_lines(data);
    if lines.is_empty() {
        return;
    }

    let longest_line_chars = lines
        .iter()
        .map(|line| line.chars().count() as i32)
        .max()
        .unwrap_or(0);
    let panel_width = longest_line_chars * GLYPH_ADVANCE + OVERLAY_PANEL_INSET_X * 2;
    let panel_height = lines.len() as i32 * LINE_ADVANCE + OVERLAY_PANEL_INSET_Y * 2;
    let panel_left = OVERLAY_PADDING - OVERLAY_PANEL_INSET_X;
    let panel_top = OVERLAY_PADDING - OVERLAY_PANEL_INSET_Y;
    draw_filled_rect(
        frame,
        width,
        height,
        panel_left,
        panel_top,
        panel_width,
        panel_height,
        OVERLAY_PANEL_BG_COLOR,
    );
    draw_rect_outline(
        frame,
        width,
        height,
        panel_left,
        panel_top,
        panel_width,
        panel_height,
        OVERLAY_PANEL_BORDER_COLOR,
    );

    let mut y = OVERLAY_PADDING;
    for line in lines {
        let color = overlay_line_color(&line);
        draw_text_clipped(frame, width, height, OVERLAY_PADDING, y, &line, color);
        y += LINE_ADVANCE;
    }
}

fn build_overlay_lines(data: &OverlayData) -> Vec<String> {
    let mut lines = vec![
        PERF_SECTION_LABEL.to_string(),
        format_fps_line(
            data.metrics.fps,
            data.render_fps_cap,
            data.slow_frame_delay_ms,
        ),
        format!("TPS: {:.1}", data.metrics.tps),
        format!("Frame: {:.2} ms", data.metrics.frame_time_ms),
        format_perf_line("SIM", data.perf.sim),
        format_perf_line("REN", data.perf.ren),
        String::new(),
        SCENE_SECTION_LABEL.to_string(),
        format!("Entities: {}", data.entity_count),
        format!("Content: {}", data.content_status),
        match data.selected_entity {
            Some(id) => format!("Sel: {}", id.0),
            None => "Sel: none".to_string(),
        },
        match data.selected_target {
            Some(target) => format!("Target: {:.1},{:.1}", target.x, target.y),
            None => "Target: idle".to_string(),
        },
        format!("items: {}", data.resource_count.unwrap_or(0)),
    ];

    if let Some(debug_info) = data.debug_info.as_ref() {
        lines.push(String::new());
        lines.push(INSPECT_SECTION_LABEL.to_string());
        lines.push(match debug_info.selected_entity {
            Some(id) => format!("sel: {}", id.0),
            None => "sel: none".to_string(),
        });
        lines.push(match debug_info.selected_position_world {
            Some(pos) => format!("pos: {:.1},{:.1}", pos.x, pos.y),
            None => "pos: none".to_string(),
        });
        lines.push(match debug_info.selected_order_world {
            Some(target) => format!("ord: {:.1},{:.1}", target.x, target.y),
            None => "ord: idle".to_string(),
        });
        lines.push(format!(
            "job: {}",
            debug_job_state_text(debug_info.selected_job_state)
        ));
        lines.push(format!(
            "cnt e/a/i/r: {}/{}/{}/{}",
            debug_info.entity_count,
            debug_info.actor_count,
            debug_info.interactable_count,
            debug_info.resource_count
        ));
        lines.push(format!("sys: {}", debug_info.system_order));
        if let Some(extra_lines) = debug_info.extra_debug_lines.as_ref() {
            for line in extra_lines {
                lines.push(line.clone());
            }
        }
    }

    lines
}

fn overlay_line_color(line: &str) -> [u8; 4] {
    if matches!(
        line,
        PERF_SECTION_LABEL | SCENE_SECTION_LABEL | INSPECT_SECTION_LABEL
    ) {
        OVERLAY_TEXT_DIM_COLOR
    } else {
        OVERLAY_TEXT_PRIMARY_COLOR
    }
}

fn format_fps_line(current_fps: f32, cap: Option<u32>, slow_frame_delay_ms: u64) -> String {
    let cap_text = match cap {
        Some(value) => value.to_string(),
        None => "inf".to_string(),
    };
    format!(
        "[{:.0} / {}] dbg+{}ms",
        current_fps, cap_text, slow_frame_delay_ms
    )
}

fn format_perf_line(label: &str, stats: super::RollingMsStats) -> String {
    format!(
        "{} l/a/m: {:.2}/{:.2}/{:.2} ms",
        label, stats.last_ms, stats.avg_ms, stats.max_ms
    )
}

fn debug_job_state_text(state: DebugJobState) -> String {
    match state {
        DebugJobState::None => "none".to_string(),
        DebugJobState::Idle => "idle".to_string(),
        DebugJobState::Working { remaining_time } => format!("work {:.1}", remaining_time),
    }
}

fn draw_text_clipped(
    frame: &mut [u8],
    width: u32,
    height: u32,
    mut x: i32,
    y: i32,
    text: &str,
    color: [u8; 4],
) {
    for ch in text.chars() {
        let glyph = glyph_for(ch).unwrap_or(SPACE_GLYPH);
        draw_glyph_clipped(frame, width, height, x, y, glyph, color);
        x += GLYPH_ADVANCE;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text_clipped_with_fallback(
    frame: &mut [u8],
    width: u32,
    height: u32,
    mut x: i32,
    y: i32,
    text: &str,
    color: [u8; 4],
    fallback_char: char,
) {
    let fallback_glyph = glyph_for(fallback_char).unwrap_or(SPACE_GLYPH);
    for ch in text.chars() {
        let glyph = glyph_for(ch).unwrap_or(fallback_glyph);
        draw_glyph_clipped(frame, width, height, x, y, glyph, color);
        x += GLYPH_ADVANCE;
    }
}

fn draw_glyph_clipped(
    frame: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    glyph: Glyph,
    color: [u8; 4],
) {
    if width == 0 || height == 0 {
        return;
    }

    let height_i32 = height as i32;
    let width_i32 = width as i32;

    for (row_index, row_bits) in glyph.rows.iter().enumerate() {
        let glyph_y = y + row_index as i32 * TEXT_SCALE;

        for col in 0..GLYPH_WIDTH {
            if (row_bits & (1 << (GLYPH_WIDTH - 1 - col))) == 0 {
                continue;
            }

            let glyph_x = x + col * TEXT_SCALE;
            for sy in 0..TEXT_SCALE {
                let pixel_y = glyph_y + sy;
                if pixel_y < 0 || pixel_y >= height_i32 {
                    continue;
                }
                for sx in 0..TEXT_SCALE {
                    let pixel_x = glyph_x + sx;
                    if pixel_x < 0 || pixel_x >= width_i32 {
                        continue;
                    }
                    write_pixel_rgba(
                        frame,
                        width as usize,
                        pixel_x as usize,
                        pixel_y as usize,
                        color,
                    );
                }
            }
        }
    }
}

fn write_pixel_rgba(frame: &mut [u8], width: usize, x: usize, y: usize, color: [u8; 4]) {
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
            write_pixel_rgba(frame, width_usize, px as usize, py as usize, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_rect_outline(
    frame: &mut [u8],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    rect_width: i32,
    rect_height: i32,
    color: [u8; 4],
) {
    if rect_width <= 1 || rect_height <= 1 {
        return;
    }
    draw_filled_rect(frame, width, height, x, y, rect_width, 1, color);
    draw_filled_rect(
        frame,
        width,
        height,
        x,
        y + rect_height - 1,
        rect_width,
        1,
        color,
    );
    draw_filled_rect(frame, width, height, x, y, 1, rect_height, color);
    draw_filled_rect(
        frame,
        width,
        height,
        x + rect_width - 1,
        y,
        1,
        rect_height,
        color,
    );
}

#[derive(Debug, Clone, Copy)]
struct Glyph {
    rows: [u8; GLYPH_HEIGHT as usize],
}

const SPACE_GLYPH: Glyph = Glyph {
    rows: [0, 0, 0, 0, 0],
};

fn glyph_for(ch: char) -> Option<Glyph> {
    match ch {
        ' '..='~' => Some(ascii_glyph(ch)),
        _ => None,
    }
}

fn ascii_glyph(ch: char) -> Glyph {
    match ch {
        ' ' => SPACE_GLYPH,
        '!' => Glyph {
            rows: [0b010, 0b010, 0b010, 0b000, 0b010],
        },
        '"' => Glyph {
            rows: [0b101, 0b101, 0b000, 0b000, 0b000],
        },
        '#' => Glyph {
            rows: [0b101, 0b111, 0b101, 0b111, 0b101],
        },
        '$' => Glyph {
            rows: [0b111, 0b110, 0b111, 0b011, 0b111],
        },
        '%' => Glyph {
            rows: [0b101, 0b001, 0b010, 0b100, 0b101],
        },
        '&' => Glyph {
            rows: [0b010, 0b101, 0b010, 0b101, 0b011],
        },
        '\'' => Glyph {
            rows: [0b010, 0b010, 0b000, 0b000, 0b000],
        },
        '(' => Glyph {
            rows: [0b001, 0b010, 0b010, 0b010, 0b001],
        },
        ')' => Glyph {
            rows: [0b100, 0b010, 0b010, 0b010, 0b100],
        },
        '*' => Glyph {
            rows: [0b000, 0b101, 0b010, 0b101, 0b000],
        },
        '+' => Glyph {
            rows: [0b000, 0b010, 0b111, 0b010, 0b000],
        },
        ',' => Glyph {
            rows: [0b000, 0b000, 0b000, 0b010, 0b100],
        },
        '-' => Glyph {
            rows: [0b000, 0b000, 0b111, 0b000, 0b000],
        },
        '.' => Glyph {
            rows: [0b000, 0b000, 0b000, 0b000, 0b010],
        },
        '/' => Glyph {
            rows: [0b001, 0b001, 0b010, 0b100, 0b100],
        },
        '0' => Glyph {
            rows: [0b111, 0b101, 0b101, 0b101, 0b111],
        },
        '1' => Glyph {
            rows: [0b010, 0b110, 0b010, 0b010, 0b111],
        },
        '2' => Glyph {
            rows: [0b111, 0b001, 0b111, 0b100, 0b111],
        },
        '3' => Glyph {
            rows: [0b111, 0b001, 0b111, 0b001, 0b111],
        },
        '4' => Glyph {
            rows: [0b101, 0b101, 0b111, 0b001, 0b001],
        },
        '5' => Glyph {
            rows: [0b111, 0b100, 0b111, 0b001, 0b111],
        },
        '6' => Glyph {
            rows: [0b111, 0b100, 0b111, 0b101, 0b111],
        },
        '7' => Glyph {
            rows: [0b111, 0b001, 0b010, 0b010, 0b010],
        },
        '8' => Glyph {
            rows: [0b111, 0b101, 0b111, 0b101, 0b111],
        },
        '9' => Glyph {
            rows: [0b111, 0b101, 0b111, 0b001, 0b111],
        },
        ':' => Glyph {
            rows: [0b000, 0b010, 0b000, 0b010, 0b000],
        },
        ';' => Glyph {
            rows: [0b000, 0b010, 0b000, 0b010, 0b100],
        },
        '<' => Glyph {
            rows: [0b001, 0b010, 0b100, 0b010, 0b001],
        },
        '=' => Glyph {
            rows: [0b000, 0b111, 0b000, 0b111, 0b000],
        },
        '>' => Glyph {
            rows: [0b100, 0b010, 0b001, 0b010, 0b100],
        },
        '?' => Glyph {
            rows: [0b111, 0b001, 0b011, 0b000, 0b010],
        },
        '@' => Glyph {
            rows: [0b111, 0b101, 0b111, 0b100, 0b111],
        },
        'A' => Glyph {
            rows: [0b010, 0b101, 0b111, 0b101, 0b101],
        },
        'B' => Glyph {
            rows: [0b110, 0b101, 0b110, 0b101, 0b110],
        },
        'C' => Glyph {
            rows: [0b111, 0b100, 0b100, 0b100, 0b111],
        },
        'D' => Glyph {
            rows: [0b110, 0b101, 0b101, 0b101, 0b110],
        },
        'E' => Glyph {
            rows: [0b111, 0b100, 0b110, 0b100, 0b111],
        },
        'F' => Glyph {
            rows: [0b111, 0b100, 0b110, 0b100, 0b100],
        },
        'G' => Glyph {
            rows: [0b111, 0b100, 0b101, 0b101, 0b111],
        },
        'H' => Glyph {
            rows: [0b101, 0b101, 0b111, 0b101, 0b101],
        },
        'I' => Glyph {
            rows: [0b111, 0b010, 0b010, 0b010, 0b111],
        },
        'J' => Glyph {
            rows: [0b111, 0b001, 0b001, 0b101, 0b111],
        },
        'K' => Glyph {
            rows: [0b101, 0b101, 0b110, 0b101, 0b101],
        },
        'L' => Glyph {
            rows: [0b100, 0b100, 0b100, 0b100, 0b111],
        },
        'M' => Glyph {
            rows: [0b101, 0b111, 0b111, 0b101, 0b101],
        },
        'N' => Glyph {
            rows: [0b101, 0b111, 0b111, 0b111, 0b101],
        },
        'O' => Glyph {
            rows: [0b111, 0b101, 0b101, 0b101, 0b111],
        },
        'P' => Glyph {
            rows: [0b110, 0b101, 0b110, 0b100, 0b100],
        },
        'Q' => Glyph {
            rows: [0b111, 0b101, 0b101, 0b111, 0b001],
        },
        'R' => Glyph {
            rows: [0b110, 0b101, 0b110, 0b101, 0b101],
        },
        'S' => Glyph {
            rows: [0b111, 0b100, 0b111, 0b001, 0b111],
        },
        'T' => Glyph {
            rows: [0b111, 0b010, 0b010, 0b010, 0b010],
        },
        'U' => Glyph {
            rows: [0b101, 0b101, 0b101, 0b101, 0b111],
        },
        'V' => Glyph {
            rows: [0b101, 0b101, 0b101, 0b101, 0b010],
        },
        'W' => Glyph {
            rows: [0b101, 0b101, 0b111, 0b111, 0b101],
        },
        'X' => Glyph {
            rows: [0b101, 0b101, 0b010, 0b101, 0b101],
        },
        'Y' => Glyph {
            rows: [0b101, 0b101, 0b010, 0b010, 0b010],
        },
        'Z' => Glyph {
            rows: [0b111, 0b001, 0b010, 0b100, 0b111],
        },
        '[' => Glyph {
            rows: [0b110, 0b100, 0b100, 0b100, 0b110],
        },
        '\\' => Glyph {
            rows: [0b100, 0b100, 0b010, 0b001, 0b001],
        },
        ']' => Glyph {
            rows: [0b011, 0b001, 0b001, 0b001, 0b011],
        },
        '^' => Glyph {
            rows: [0b010, 0b101, 0b000, 0b000, 0b000],
        },
        '_' => Glyph {
            rows: [0b000, 0b000, 0b000, 0b000, 0b111],
        },
        '`' => Glyph {
            rows: [0b100, 0b010, 0b000, 0b000, 0b000],
        },
        'a' => Glyph {
            rows: [0b000, 0b111, 0b001, 0b111, 0b111],
        },
        'b' => Glyph {
            rows: [0b100, 0b100, 0b110, 0b101, 0b110],
        },
        'c' => Glyph {
            rows: [0b000, 0b111, 0b100, 0b100, 0b111],
        },
        'd' => Glyph {
            rows: [0b001, 0b001, 0b111, 0b101, 0b111],
        },
        'e' => Glyph {
            rows: [0b000, 0b111, 0b110, 0b100, 0b111],
        },
        'f' => Glyph {
            rows: [0b011, 0b100, 0b110, 0b100, 0b100],
        },
        'g' => Glyph {
            rows: [0b000, 0b111, 0b101, 0b111, 0b001],
        },
        'h' => Glyph {
            rows: [0b100, 0b100, 0b110, 0b101, 0b101],
        },
        'i' => Glyph {
            rows: [0b010, 0b000, 0b010, 0b010, 0b010],
        },
        'j' => Glyph {
            rows: [0b001, 0b000, 0b001, 0b101, 0b010],
        },
        'k' => Glyph {
            rows: [0b100, 0b101, 0b110, 0b101, 0b101],
        },
        'l' => Glyph {
            rows: [0b100, 0b100, 0b100, 0b100, 0b111],
        },
        'm' => Glyph {
            rows: [0b000, 0b110, 0b111, 0b101, 0b101],
        },
        'n' => Glyph {
            rows: [0b000, 0b110, 0b101, 0b101, 0b101],
        },
        'o' => Glyph {
            rows: [0b000, 0b111, 0b101, 0b101, 0b111],
        },
        'p' => Glyph {
            rows: [0b000, 0b110, 0b101, 0b110, 0b100],
        },
        'q' => Glyph {
            rows: [0b000, 0b111, 0b101, 0b111, 0b001],
        },
        'r' => Glyph {
            rows: [0b000, 0b110, 0b101, 0b100, 0b100],
        },
        's' => Glyph {
            rows: [0b000, 0b111, 0b110, 0b001, 0b111],
        },
        't' => Glyph {
            rows: [0b010, 0b111, 0b010, 0b010, 0b011],
        },
        'u' => Glyph {
            rows: [0b000, 0b101, 0b101, 0b101, 0b111],
        },
        'v' => Glyph {
            rows: [0b000, 0b101, 0b101, 0b101, 0b010],
        },
        'w' => Glyph {
            rows: [0b000, 0b101, 0b101, 0b111, 0b010],
        },
        'x' => Glyph {
            rows: [0b000, 0b101, 0b010, 0b010, 0b101],
        },
        'y' => Glyph {
            rows: [0b000, 0b101, 0b101, 0b111, 0b001],
        },
        'z' => Glyph {
            rows: [0b000, 0b111, 0b001, 0b010, 0b111],
        },
        '{' => Glyph {
            rows: [0b011, 0b010, 0b110, 0b010, 0b011],
        },
        '|' => Glyph {
            rows: [0b010, 0b010, 0b010, 0b010, 0b010],
        },
        '}' => Glyph {
            rows: [0b110, 0b010, 0b011, 0b010, 0b110],
        },
        '~' => Glyph {
            rows: [0b000, 0b011, 0b110, 0b000, 0b000],
        },
        _ => SPACE_GLYPH,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_lookup_covers_ascii_printable_range() {
        for code in 32u8..=126u8 {
            let ch = char::from(code);
            assert!(
                glyph_for(ch).is_some(),
                "missing glyph for ASCII code {code} ('{ch}')"
            );
        }
    }

    #[test]
    fn non_ascii_printable_glyphs_use_fallback_path() {
        assert!(glyph_for('\u{7f}').is_none());
        assert!(glyph_for('é').is_none());
    }

    #[test]
    fn console_acceptance_ascii_string_has_no_fallbacks() {
        let acceptance = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 !\\\"#$%&'()*+,-./:;<=>?@[\\\\]^_`{|}~";
        for ch in acceptance.chars() {
            assert!(
                glyph_for(ch).is_some(),
                "acceptance character should have glyph: '{ch}'"
            );
        }
    }

    #[test]
    fn unknown_character_is_safe_and_draws_like_space() {
        let mut frame = vec![0u8; 16 * 16 * 4];
        draw_text_clipped(
            &mut frame,
            16,
            16,
            0,
            0,
            "\u{1f642}",
            OVERLAY_TEXT_PRIMARY_COLOR,
        );
        assert!(frame.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn clipped_glyph_draw_with_negative_origin_is_safe() {
        let mut frame = vec![0u8; 8 * 8 * 4];
        draw_text_clipped(&mut frame, 8, 8, -2, -2, "FPS", OVERLAY_TEXT_PRIMARY_COLOR);
        assert_eq!(frame.len(), 8 * 8 * 4);
    }

    #[test]
    fn clipped_glyph_draw_beyond_bounds_is_safe() {
        let mut frame = vec![0u8; 8 * 8 * 4];
        draw_text_clipped(&mut frame, 8, 8, 64, 64, "TPS", OVERLAY_TEXT_PRIMARY_COLOR);
        assert!(frame.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn tiny_viewports_never_panic_or_write_oob() {
        let mut frame_1x1 = vec![0u8; 4];
        draw_text_clipped(
            &mut frame_1x1,
            1,
            1,
            -10,
            -10,
            "Frame",
            OVERLAY_TEXT_PRIMARY_COLOR,
        );

        let mut frame_0x8 = vec![];
        draw_text_clipped(
            &mut frame_0x8,
            0,
            8,
            0,
            0,
            "Entities",
            OVERLAY_TEXT_PRIMARY_COLOR,
        );

        let mut frame_8x0 = vec![];
        draw_text_clipped(
            &mut frame_8x0,
            8,
            0,
            0,
            0,
            "Content",
            OVERLAY_TEXT_PRIMARY_COLOR,
        );
    }

    #[test]
    fn layout_metrics_follow_text_scale() {
        assert_eq!(TEXT_SCALE, 3);
        assert_eq!(GLYPH_ADVANCE, 12);
        assert_eq!(LINE_ADVANCE, 21);
        assert_eq!(OVERLAY_PADDING, 18);
    }

    #[test]
    fn inspect_block_lines_follow_scaled_layout() {
        let data = OverlayData {
            metrics: LoopMetricsSnapshot::default(),
            perf: PerfStatsSnapshot::default(),
            render_fps_cap: Some(240),
            slow_frame_delay_ms: 0,
            entity_count: 3,
            content_status: "loaded",
            selected_entity: Some(EntityId(1)),
            selected_target: None,
            resource_count: Some(2),
            debug_info: Some(DebugInfoSnapshot {
                selected_entity: Some(EntityId(1)),
                selected_position_world: Some(crate::app::Vec2 { x: 1.0, y: 2.0 }),
                selected_order_world: None,
                selected_job_state: DebugJobState::Working {
                    remaining_time: 1.5,
                },
                entity_count: 3,
                actor_count: 1,
                interactable_count: 1,
                resource_count: 2,
                system_order: "InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup"
                    .to_string(),
                extra_debug_lines: Some(vec![
                    "ev: 1".to_string(),
                    "evk: is:0 ic:0 dm:0 dd:0 sa:1 se:0".to_string(),
                ]),
            }),
        };
        let lines = build_overlay_lines(&data);
        assert_eq!(lines.len(), 23);
        assert_eq!(lines[14], INSPECT_SECTION_LABEL);
        assert_eq!(
            lines[20],
            "sys: InputIntent>Interaction>AI>CombatResolution>StatusEffects>Cleanup"
        );
        assert_eq!(lines[21], "ev: 1");
        assert_eq!(lines[22], "evk: is:0 ic:0 dm:0 dd:0 sa:1 se:0");
        assert_eq!(
            OVERLAY_PADDING + (lines.len() as i32 - 1) * LINE_ADVANCE,
            480
        );
    }

    #[test]
    fn draw_overlay_writes_backing_plate_pixels() {
        let data = OverlayData {
            metrics: LoopMetricsSnapshot::default(),
            perf: PerfStatsSnapshot::default(),
            render_fps_cap: Some(60),
            slow_frame_delay_ms: 0,
            entity_count: 1,
            content_status: "loaded",
            selected_entity: None,
            selected_target: None,
            resource_count: Some(0),
            debug_info: None,
        };
        let mut frame = vec![0u8; 320 * 180 * 4];
        draw_overlay(&mut frame, 320, 180, &data);

        let has_backing_pixel = frame.chunks_exact(4).any(|px| {
            px[0] == OVERLAY_PANEL_BG_COLOR[0]
                && px[1] == OVERLAY_PANEL_BG_COLOR[1]
                && px[2] == OVERLAY_PANEL_BG_COLOR[2]
                && px[3] == OVERLAY_PANEL_BG_COLOR[3]
        });
        assert!(has_backing_pixel);
    }

    #[test]
    fn fps_line_formats_cap_on_and_debug_delay() {
        let line = format_fps_line(144.4, Some(240), 200);
        assert_eq!(line, "[144 / 240] dbg+200ms");
    }

    #[test]
    fn fps_line_formats_cap_off_with_ascii_text() {
        let line = format_fps_line(144.4, None, 0);
        assert_eq!(line, "[144 / inf] dbg+0ms");
    }

    #[test]
    fn perf_line_formats_last_avg_max() {
        let line = format_perf_line(
            "SIM",
            super::super::RollingMsStats {
                last_ms: 1.25,
                avg_ms: 2.5,
                max_ms: 5.75,
            },
        );
        assert_eq!(line, "SIM l/a/m: 1.25/2.50/5.75 ms");
    }
}
