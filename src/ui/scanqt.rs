use super::standby::draw_frequency;
use super::TextBuf;
use crate::app;
use crate::device::radio::SubAudio;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

fn tone_text<W: core::fmt::Write>(w: &mut W, tone: Option<SubAudio>) {
    match tone {
        None | Some(SubAudio::None) => {
            let _ = write!(w, "NONE");
        }
        Some(SubAudio::Ctcss(hz)) => {
            let _ = write!(w, "{}.{}Hz", hz / 10, hz % 10);
        }
        Some(SubAudio::Dcs { code, inverted }) => {
            let _ = write!(w, "D{:03o}{}", code, if inverted { "I" } else { "N" });
        }
    }
}

pub fn draw_scanqt<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    Text::new("QT SCAN", Point::new(4, 12), small).draw(lcd).ok();

    draw_frequency(lcd, app.watching_freq_hz(), 36);

    let status = if app.scanqt_is_found() {
        "FOUND"
    } else if app.scanqt_is_listening() {
        "DETECTING..."
    } else {
        "WAITING..."
    };
    Text::new(status, Point::new(4, 50), small).draw(lcd).ok();

    if app.scanqt_is_found() {
        let mut tone_line: TextBuf<16> = TextBuf::new();
        tone_text(&mut tone_line, app.scanqt_tone());
        Text::new(tone_line.as_str(), Point::new(60, 50), small)
            .draw(lcd)
            .ok();
    }

    Text::new("MENU SAVE  EXIT", Point::new(2, 62), small)
        .draw(lcd)
        .ok();
}
