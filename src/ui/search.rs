use super::standby::draw_frequency;
use super::TextBuf;
use crate::app::{self, SearchStatus};
use crate::device::radio::SubAudio;
use core::fmt::Write as _;
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

pub fn draw_search<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    let mut header: TextBuf<20> = TextBuf::new();
    write!(header, "SEARCH {}", app.search_band_label()).ok();
    Text::new(header.as_str(), Point::new(4, 12), small)
        .draw(lcd)
        .ok();

    match app.search_status() {
        SearchStatus::Hunting => {
            Text::new("HUNTING...", Point::new(4, 40), small)
                .draw(lcd)
                .ok();
        }
        SearchStatus::Listening => {
            draw_frequency(lcd, app.search_candidate_freq_hz(), 40);
            Text::new("LISTEN", Point::new(4, 52), small).draw(lcd).ok();
        }
        SearchStatus::Found => {
            draw_frequency(lcd, app.search_candidate_freq_hz(), 30);
            let mut tone_line: TextBuf<16> = TextBuf::new();
            tone_text(&mut tone_line, app.search_tone());
            Text::new(tone_line.as_str(), Point::new(4, 52), small)
                .draw(lcd)
                .ok();
        }
    }

    Text::new("AB BAND  MENU SAVE", Point::new(2, 62), small)
        .draw(lcd)
        .ok();
}
