use super::standby::draw_frequency;
use super::TextBuf;
use crate::app;
use core::fmt::Write as _;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

pub fn draw_scan<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    let mut header: TextBuf<20> = TextBuf::new();
    let dir = if app.scan_direction_up() { "UP" } else { "DN" };
    if app.watching_is_channel_mode() {
        write!(header, "SCAN {} M{}", dir, app.watching_channel_num()).ok();
    } else {
        write!(header, "SCAN {}", dir).ok();
    }
    Text::new(header.as_str(), Point::new(4, 12), small)
        .draw(lcd)
        .ok();

    draw_frequency(lcd, app.watching_freq_hz(), 40);

    Text::new("UP/DN STEP  #/EXIT STOP", Point::new(2, 62), small)
        .draw(lcd)
        .ok();
}
