//! Static "programming in progress" screen shown for the duration of a CPS
//! write session (the session loop takes over the whole MCU, so this is
//! drawn once on entry and never redrawn).

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

pub fn draw_programming<D>(target: &mut D)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(target)
        .ok();

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("AURA CPS", Point::new(20, 26), style)
        .draw(target)
        .ok();
    Text::new("PROGRAMMING...", Point::new(8, 40), style)
        .draw(target)
        .ok();
}
