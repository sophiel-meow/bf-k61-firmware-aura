//! Status-bar icons: battery, antenna/signal bars, VFO markers, key-lock.
//!
//! Pixel data ported from `uv-k5-firmware-custom` (bitmaps.c), which is
//! Copyright DualTachyon / egzumer / F4HWN, licensed Apache License 2.0
//! (http://www.apache.org/licenses/LICENSE-2.0). Original data is packed one
//! byte per 8px column (ST7565 page format); converted offline to row-major
//! MSB-first packing for `embedded_graphics::image::ImageRaw` by
//! `tools/convert_bitmaps.py` — no per-frame bit shuffling at runtime.

use embedded_graphics::image::ImageRaw;
use embedded_graphics::pixelcolor::BinaryColor;

pub const BATTERY_OUTLINE_WIDTH: u32 = 17;
const BATTERY_OUTLINE_DATA: [u8; 24] = [
    0x1f, 0xff, 0x80, 0x60, 0x00, 0x80, 0x40, 0x00, 0x80, 0x40, 0x00, 0x80, 0x40, 0x00, 0x80, 0x60,
    0x00, 0x80, 0x1f, 0xff, 0x80, 0x00, 0x00, 0x00,
];
pub fn battery_outline() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&BATTERY_OUTLINE_DATA, BATTERY_OUTLINE_WIDTH)
}

pub const BATTERY_FILL_SEGMENT_WIDTH: u32 = 2;
const BATTERY_FILL_SEGMENT_DATA: [u8; 8] = [0xc0, 0x00, 0xc0, 0xc0, 0xc0, 0x00, 0xc0, 0x00];
pub fn battery_fill_segment() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&BATTERY_FILL_SEGMENT_DATA, BATTERY_FILL_SEGMENT_WIDTH)
}

pub const ANTENNA_WIDTH: u32 = 5;
const ANTENNA_DATA: [u8; 8] = [0xf8, 0xa8, 0x70, 0x20, 0x20, 0x20, 0x20, 0x00];
pub fn antenna() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&ANTENNA_DATA, ANTENNA_WIDTH)
}

pub const VFO_DEFAULT_WIDTH: u32 = 7;
const VFO_DEFAULT_DATA: [u8; 8] = [0xc0, 0xf0, 0xfc, 0xfe, 0xfc, 0xf0, 0xc0, 0x00];
pub fn vfo_default() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&VFO_DEFAULT_DATA, VFO_DEFAULT_WIDTH)
}

pub const VFO_NOT_DEFAULT_WIDTH: u32 = 7;
const VFO_NOT_DEFAULT_DATA: [u8; 8] = [0xc0, 0x30, 0x0c, 0x02, 0x0c, 0x30, 0xc0, 0x00];
pub fn vfo_not_default() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&VFO_NOT_DEFAULT_DATA, VFO_NOT_DEFAULT_WIDTH)
}

pub const VFO_LOCK_WIDTH: u32 = 7;
const VFO_LOCK_DATA: [u8; 8] = [0x38, 0x44, 0xfe, 0x82, 0x82, 0x82, 0xfe, 0x00];
pub fn vfo_lock() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&VFO_LOCK_DATA, VFO_LOCK_WIDTH)
}

pub const KEY_LOCK_WIDTH: u32 = 9;
const KEY_LOCK_DATA: [u8; 16] = [
    0x3e, 0x00, 0x41, 0x00, 0xff, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xff, 0x80, 0x00, 0x00,
];
pub fn key_lock() -> ImageRaw<'static, BinaryColor> {
    ImageRaw::new(&KEY_LOCK_DATA, KEY_LOCK_WIDTH)
}

/// Draws the battery gauge (outline + up to 4 fill segments):
/// <2 empty,
/// 2 = outline only,
/// 3-6 = 1-4 fill segments.
pub fn draw_battery<D>(target: &mut D, right_x: i32, y: i32, level: u8)
where
    D: embedded_graphics::draw_target::DrawTarget<Color = BinaryColor>,
{
    use embedded_graphics::image::Image;
    use embedded_graphics::prelude::*;

    let x = right_x - BATTERY_OUTLINE_WIDTH as i32;
    let outline = battery_outline();
    Image::new(&outline, Point::new(x, y)).draw(target).ok();

    let bars = level.saturating_sub(2).min(4);
    let fill = battery_fill_segment();
    for slot in (4 - bars)..4 {
        let fx = x + 2 + slot as i32 * 3;
        Image::new(&fill, Point::new(fx, y)).draw(target).ok();
    }
}
