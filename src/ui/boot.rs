use super::TextBuf;
use crate::flash_map::Settings;
use core::fmt::Write as _;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics::Pixel;
use profont::PROFONT_14_POINT;

const FIRMWARE_VERSION: &str = env!("GIT_VERSION");

fn ascii_line(raw: &[u8; 16]) -> &str {
    let end = raw
        .iter()
        .position(|&b| b == 0x00 || b == 0xFF)
        .unwrap_or(raw.len());
    core::str::from_utf8(&raw[..end]).unwrap_or("")
}

fn clear<D>(lcd: &mut D)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();
}

/// Falls back to a generic greeting when the user hasn't set custom text.
fn text_lines(settings: &Settings) -> (&str, &str) {
    let l1 = ascii_line(&settings.boot_text_line1);
    let l2 = ascii_line(&settings.boot_text_line2);
    if l1.is_empty() && l2.is_empty() {
        ("UV-K6x", "Aura")
    } else {
        (l1, l2)
    }
}

/// Boot display mode 3: stored logo bitmap.
pub fn draw_logo<D>(lcd: &mut D, page_buf: &[u8; crate::flash_map::BOOT_LOGO_SIZE])
where
    D: DrawTarget<Color = BinaryColor>,
{
    clear(lcd);
    for page in 0..8usize {
        let row = &page_buf[page * 128..page * 128 + 128];
        let pixels = row.iter().enumerate().flat_map(move |(col, &byte)| {
            (0..8usize)
                .filter(move |bit| byte & (1 << bit) != 0)
                .map(move |bit| {
                    Pixel(
                        Point::new(col as i32, (page * 8 + bit) as i32),
                        BinaryColor::On,
                    )
                })
        });
        lcd.draw_iter(pixels).ok();
    }
}

fn draw_centered_big<D>(lcd: &mut D, text: &str, baseline_y: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let width = text.chars().count() as i32 * PROFONT_14_POINT.character_size.width as i32;
    let x = (128 - width) / 2;
    Text::new(
        text,
        Point::new(x, baseline_y),
        MonoTextStyle::new(&PROFONT_14_POINT, BinaryColor::On),
    )
    .draw(lcd)
    .ok();
}

fn draw_centered_small<D>(lcd: &mut D, text: &str, baseline_y: i32)
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

fn draw_firmware_banner<D>(lcd: &mut D, text: &str, top: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    const PAD: i32 = 6;
    let text_width = text.chars().count() as i32 * FONT_6X10.character_size.width as i32;
    let width = text_width + PAD * 2;
    let x = (128 - width) / 2;
    let height = FONT_6X10.character_size.height;
    Rectangle::new(Point::new(x, top), Size::new(width as u32, height))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(lcd)
        .ok();
    Text::new(
        text,
        Point::new(x + PAD, top + FONT_6X10.baseline as i32),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off),
    )
    .draw(lcd)
    .ok();
}

fn draw_footer<D>(lcd: &mut D)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Line::new(Point::new(0, 40), Point::new(127, 40))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(lcd)
        .ok();

    draw_firmware_banner(lcd, "UV-K6x Aura", 40);

    draw_centered_small(lcd, FIRMWARE_VERSION, 61);
}

fn draw_headlines<D>(lcd: &mut D, l1: &str, l2: &str)
where
    D: DrawTarget<Color = BinaryColor>,
{
    clear(lcd);
    draw_centered_big(lcd, l1, 20);
    draw_centered_big(lcd, l2, 35);
    draw_footer(lcd);
}

/// Boot display mode 2: two lines of custom text.
pub fn draw_message<D>(lcd: &mut D, settings: &Settings)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let (l1, l2) = text_lines(settings);
    draw_headlines(lcd, l1, l2);
}

/// Boot display mode 1: "VOLTAGE" / actual battery voltage, f4hwn-style.
pub fn draw_voltage<D>(lcd: &mut D, voltage_cv: u16)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut line: TextBuf<16> = TextBuf::new();
    write!(line, "{}.{:02}V", voltage_cv / 100, voltage_cv % 100).ok();
    draw_headlines(lcd, "VOLTAGE", line.as_str());
}
