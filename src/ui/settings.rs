use super::TextBuf;
use crate::app::{self, ChannelDisplayMode};
use crate::device::radio::{Power, SubAudio};
use core::fmt::Write as _;
use embedded_graphics::image::Image;
use embedded_graphics::mono_font::{
    ascii::{FONT_5X8, FONT_6X10},
    MonoTextStyle,
};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use profont::PROFONT_14_POINT;

// draw_settings
pub fn draw_settings<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    Text::new(
        app.settings_item_label(),
        Point::new(4, 14),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
    )
    .draw(lcd)
    .ok();

    let mut value: TextBuf<20> = TextBuf::new();
    app.settings_value_text(&mut value);

    let editing = app.settings_editing();
    let (bg, fg) = if editing {
        (BinaryColor::On, BinaryColor::Off)
    } else {
        (BinaryColor::Off, BinaryColor::On)
    };
    Rectangle::new(Point::new(0, 30), Size::new(128, 20))
        .into_styled(PrimitiveStyle::with_fill(bg))
        .draw(lcd)
        .ok();
    Text::new(
        value.as_str(),
        Point::new(4, 44),
        MonoTextStyle::new(&FONT_6X10, fg),
    )
    .draw(lcd)
    .ok();
}
