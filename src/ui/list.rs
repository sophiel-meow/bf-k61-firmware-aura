use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, Triangle};
use embedded_graphics::text::Text;

use super::TextBuf;

const VISIBLE_ROWS: usize = 5;
const ROW_HEIGHT: i32 = FONT_6X10.character_size.height as i32;
const LIST_TOP: i32 = ROW_HEIGHT;
const RIGHT_MARGIN: i32 = 4;

const ARROW_W: i32 = 6;
const ARROW_H: i32 = 5;
const ARROW_GAP: i32 = 5;

pub(super) trait ListSource {
    fn row_count(&mut self) -> usize;
    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write);
    fn value(&mut self, _index: usize, _w: &mut dyn core::fmt::Write) -> bool {
        false
    }
    fn cursor(&mut self, _index: usize) -> Option<usize> {
        None
    }
}

pub(super) fn draw_list<D>(
    lcd: &mut D,
    title: &str,
    source: &mut dyn ListSource,
    selected: usize,
    show_arrows: bool,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    draw_centered(lcd, title, FONT_6X10.baseline as i32);

    let total = source.row_count();
    let top = scroll_top(total, selected);

    for slot in 0..VISIBLE_ROWS {
        let index = top + slot;
        if index >= total {
            break;
        }

        let row_top = LIST_TOP + slot as i32 * ROW_HEIGHT;
        let is_selected = index == selected;

        if is_selected {
            Rectangle::new(Point::new(0, row_top), Size::new(128, ROW_HEIGHT as u32))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(lcd)
                .ok();
        }
        let fg = if is_selected {
            BinaryColor::Off
        } else {
            BinaryColor::On
        };
        let baseline = row_top + FONT_6X10.baseline as i32;

        let mut label: TextBuf<16> = TextBuf::new();
        source.label(index, &mut label);
        Text::new(
            label.as_str(),
            Point::new(4, baseline),
            MonoTextStyle::new(&FONT_6X10, fg),
        )
        .draw(lcd)
        .ok();

        let mut value: TextBuf<14> = TextBuf::new();
        if source.value(index, &mut value) {
            if let Some(cursor) = source.cursor(index) {
                draw_value_with_cursor(lcd, value.as_str(), cursor, row_top, baseline, fg);
            } else if is_selected && show_arrows {
                draw_editing_value(lcd, value.as_str(), row_top, fg);
            } else {
                draw_value(lcd, value.as_str(), baseline, fg);
            }
        }
    }
}

/// Keeps `selected` inside a `VISIBLE_ROWS`-tall window, centered where
/// possible but clamped so the window never scrolls past the list ends.
fn scroll_top(total: usize, selected: usize) -> usize {
    if total <= VISIBLE_ROWS {
        return 0;
    }
    let half = VISIBLE_ROWS / 2;
    selected.saturating_sub(half).min(total - VISIBLE_ROWS)
}

fn draw_centered<D>(lcd: &mut D, text: &str, baseline_y: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let width = text.chars().count() as i32 * FONT_6X10.character_size.width as i32;
    let x = (128 - width) / 2;
    Text::new(
        text,
        Point::new(x, baseline_y),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
    )
    .draw(lcd)
    .ok();
}

fn draw_value<D>(lcd: &mut D, text: &str, baseline_y: i32, fg: BinaryColor)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let width = text.chars().count() as i32 * FONT_6X10.character_size.width as i32;
    let x = 128 - RIGHT_MARGIN - width;
    Text::new(
        text,
        Point::new(x, baseline_y),
        MonoTextStyle::new(&FONT_6X10, fg),
    )
    .draw(lcd)
    .ok();
}

fn draw_value_with_cursor<D>(
    lcd: &mut D,
    text: &str,
    cursor_index: usize,
    row_top: i32,
    baseline_y: i32,
    fg: BinaryColor,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    draw_value(lcd, text, baseline_y, fg);

    let char_w = FONT_6X10.character_size.width as i32;
    let width = text.chars().count() as i32 * char_w;
    let x0 = 128 - RIGHT_MARGIN - width;

    if let Some(ch) = text.chars().nth(cursor_index) {
        let cursor_x = x0 + cursor_index as i32 * char_w;
        let inverted = if fg == BinaryColor::On {
            BinaryColor::Off
        } else {
            BinaryColor::On
        };
        Rectangle::new(
            Point::new(cursor_x, row_top),
            Size::new(char_w as u32, ROW_HEIGHT as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(fg))
        .draw(lcd)
        .ok();

        let mut ch_buf = [0u8; 4];
        Text::new(
            ch.encode_utf8(&mut ch_buf),
            Point::new(cursor_x, baseline_y),
            MonoTextStyle::new(&FONT_6X10, inverted),
        )
        .draw(lcd)
        .ok();
    }
}

fn draw_editing_value<D>(lcd: &mut D, text: &str, row_top: i32, fg: BinaryColor)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let value_width = text.chars().count() as i32 * FONT_6X10.character_size.width as i32;
    let total_width = ARROW_W + ARROW_GAP + value_width + ARROW_GAP + ARROW_W;
    let start_x = 128 - RIGHT_MARGIN - total_width;
    let mid_y = row_top + ROW_HEIGHT / 2;

    draw_triangle_up(lcd, start_x + ARROW_W / 2, mid_y, fg);

    let value_x = start_x + ARROW_W + ARROW_GAP;
    let baseline = row_top + FONT_6X10.baseline as i32;
    Text::new(
        text,
        Point::new(value_x, baseline),
        MonoTextStyle::new(&FONT_6X10, fg),
    )
    .draw(lcd)
    .ok();

    let down_cx = value_x + value_width + ARROW_GAP + ARROW_W / 2;
    draw_triangle_down(lcd, down_cx, mid_y, fg);
}

fn draw_triangle_up<D>(lcd: &mut D, cx: i32, cy: i32, fg: BinaryColor)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Triangle::new(
        Point::new(cx, cy - ARROW_H / 2),
        Point::new(cx - ARROW_W / 2, cy + ARROW_H / 2),
        Point::new(cx + ARROW_W / 2, cy + ARROW_H / 2),
    )
    .into_styled(PrimitiveStyle::with_fill(fg))
    .draw(lcd)
    .ok();
}

fn draw_triangle_down<D>(lcd: &mut D, cx: i32, cy: i32, fg: BinaryColor)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Triangle::new(
        Point::new(cx, cy + ARROW_H / 2),
        Point::new(cx - ARROW_W / 2, cy - ARROW_H / 2),
        Point::new(cx + ARROW_W / 2, cy - ARROW_H / 2),
    )
    .into_styled(PrimitiveStyle::with_fill(fg))
    .draw(lcd)
    .ok();
}
