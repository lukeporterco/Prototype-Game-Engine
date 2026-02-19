use crate::app::LoopMetricsSnapshot;

const GLYPH_WIDTH: i32 = 3;
const GLYPH_HEIGHT: i32 = 5;
const GLYPH_ADVANCE: i32 = GLYPH_WIDTH + 1;
const LINE_ADVANCE: i32 = GLYPH_HEIGHT + 2;
const OVERLAY_PADDING: i32 = 6;
const OVERLAY_COLOR: [u8; 4] = [230, 240, 180, 255];

#[derive(Debug, Clone, Copy)]
pub(crate) struct OverlayData {
    pub metrics: LoopMetricsSnapshot,
    pub entity_count: usize,
    pub content_status: &'static str,
}

pub(crate) fn draw_overlay(frame: &mut [u8], width: u32, height: u32, data: &OverlayData) {
    if width == 0 || height == 0 {
        return;
    }

    let lines = [
        format!("FPS: {:.1}", data.metrics.fps),
        format!("TPS: {:.1}", data.metrics.tps),
        format!("Frame: {:.2} ms", data.metrics.frame_time_ms),
        format!("Entities: {}", data.entity_count),
        format!("Content: {}", data.content_status),
    ];

    let mut y = OVERLAY_PADDING;
    for line in lines {
        draw_text_clipped(
            frame,
            width,
            height,
            OVERLAY_PADDING,
            y,
            &line,
            OVERLAY_COLOR,
        );
        y += LINE_ADVANCE;
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
        let pixel_y = y + row_index as i32;
        if pixel_y < 0 || pixel_y >= height_i32 {
            continue;
        }

        for col in 0..GLYPH_WIDTH {
            if (row_bits & (1 << (GLYPH_WIDTH - 1 - col))) == 0 {
                continue;
            }

            let pixel_x = x + col;
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

#[derive(Debug, Clone, Copy)]
struct Glyph {
    rows: [u8; GLYPH_HEIGHT as usize],
}

const SPACE_GLYPH: Glyph = Glyph {
    rows: [0, 0, 0, 0, 0],
};

fn glyph_for(ch: char) -> Option<Glyph> {
    Some(match ch {
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
        '.' => Glyph {
            rows: [0b000, 0b000, 0b000, 0b000, 0b010],
        },
        ':' => Glyph {
            rows: [0b000, 0b010, 0b000, 0b010, 0b000],
        },
        ' ' => SPACE_GLYPH,
        '-' => Glyph {
            rows: [0b000, 0b000, 0b111, 0b000, 0b000],
        },
        'F' => Glyph {
            rows: [0b111, 0b100, 0b110, 0b100, 0b100],
        },
        'P' => Glyph {
            rows: [0b110, 0b101, 0b110, 0b100, 0b100],
        },
        'S' => Glyph {
            rows: [0b111, 0b100, 0b111, 0b001, 0b111],
        },
        'T' => Glyph {
            rows: [0b111, 0b010, 0b010, 0b010, 0b010],
        },
        'E' => Glyph {
            rows: [0b111, 0b100, 0b110, 0b100, 0b111],
        },
        'C' => Glyph {
            rows: [0b111, 0b100, 0b100, 0b100, 0b111],
        },
        'r' => Glyph {
            rows: [0b000, 0b110, 0b101, 0b100, 0b100],
        },
        'a' => Glyph {
            rows: [0b000, 0b111, 0b001, 0b111, 0b111],
        },
        'm' => Glyph {
            rows: [0b000, 0b110, 0b111, 0b101, 0b101],
        },
        'e' => Glyph {
            rows: [0b000, 0b111, 0b110, 0b100, 0b111],
        },
        'n' => Glyph {
            rows: [0b000, 0b110, 0b101, 0b101, 0b101],
        },
        't' => Glyph {
            rows: [0b010, 0b111, 0b010, 0b010, 0b011],
        },
        'i' => Glyph {
            rows: [0b010, 0b000, 0b010, 0b010, 0b010],
        },
        's' => Glyph {
            rows: [0b000, 0b111, 0b110, 0b001, 0b111],
        },
        'o' => Glyph {
            rows: [0b000, 0b111, 0b101, 0b101, 0b111],
        },
        'l' => Glyph {
            rows: [0b100, 0b100, 0b100, 0b100, 0b111],
        },
        'd' => Glyph {
            rows: [0b001, 0b001, 0b111, 0b101, 0b111],
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn glyph_lookup_covers_exact_required_char_set() {
        let supported: HashSet<char> = (32u8..=126u8)
            .map(char::from)
            .filter(|ch| glyph_for(*ch).is_some())
            .collect();

        let required: HashSet<char> = [
            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', ':', ' ', '-', 'F', 'P', 'S',
            'T', 'r', 'a', 'm', 'e', 'E', 'n', 't', 'i', 's', 'C', 'o', 'l', 'd',
        ]
        .into_iter()
        .collect();

        assert_eq!(supported, required);
    }

    #[test]
    fn unknown_character_is_safe_and_draws_like_space() {
        let mut frame = vec![0u8; 16 * 16 * 4];
        draw_text_clipped(&mut frame, 16, 16, 0, 0, "@", OVERLAY_COLOR);
        assert!(frame.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn clipped_glyph_draw_with_negative_origin_is_safe() {
        let mut frame = vec![0u8; 8 * 8 * 4];
        draw_text_clipped(&mut frame, 8, 8, -2, -2, "FPS", OVERLAY_COLOR);
        assert_eq!(frame.len(), 8 * 8 * 4);
    }

    #[test]
    fn clipped_glyph_draw_beyond_bounds_is_safe() {
        let mut frame = vec![0u8; 8 * 8 * 4];
        draw_text_clipped(&mut frame, 8, 8, 64, 64, "TPS", OVERLAY_COLOR);
        assert!(frame.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn tiny_viewports_never_panic_or_write_oob() {
        let mut frame_1x1 = vec![0u8; 4];
        draw_text_clipped(&mut frame_1x1, 1, 1, -10, -10, "Frame", OVERLAY_COLOR);

        let mut frame_0x8 = vec![];
        draw_text_clipped(&mut frame_0x8, 0, 8, 0, 0, "Entities", OVERLAY_COLOR);

        let mut frame_8x0 = vec![];
        draw_text_clipped(&mut frame_8x0, 8, 0, 0, 0, "Content", OVERLAY_COLOR);
    }
}
