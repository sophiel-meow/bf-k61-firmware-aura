use super::list::{draw_list, ListSource};
use super::standby::{draw_frequency, draw_right_aligned};
use super::TextBuf;
use crate::app;
use core::fmt::Write as _;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use profont::PROFONT_14_POINT;

#[rustfmt::skip]
const CHANNEL_LABELS: [&str; 30] = [
    "CH 01", "CH 02", "CH 03", "CH 04", "CH 05", "CH 06", "CH 07", "CH 08", "CH 09", "CH 10",
    "CH 11", "CH 12", "CH 13", "CH 14", "CH 15", "CH 16", "CH 17", "CH 18", "CH 19", "CH 20",
    "CH 21", "CH 22", "CH 23", "CH 24", "CH 25", "CH 26", "CH 27", "CH 28", "CH 29", "CH 30",
];

struct FmSaveSource<'a>(&'a app::App);

impl<'a> ListSource for FmSaveSource<'a> {
    fn row_count(&mut self) -> usize {
        crate::flash_map::FM_CHANNEL_COUNT
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        let _ = write!(w, "{}", CHANNEL_LABELS[index]);
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        match self.0.fm_channel_freq_at(index) {
            Some(deci_mhz) => {
                let _ = write!(w, "{}.{}", deci_mhz / 10, deci_mhz % 10);
            }
            None => {
                let _ = write!(w, "EMPTY");
            }
        }
        true
    }
}

pub fn draw_fm<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    if app.fm_save_picker_selected().is_some() {
        draw_save_picker(lcd, app);
    } else {
        draw_tuning(lcd, app);
    }
}

fn draw_save_picker<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut source = FmSaveSource(app);
    let selected = app.fm_save_picker_selected().unwrap_or(0) as usize;
    draw_list(lcd, "SAVE TO", &mut source, selected, false);
}

fn draw_tuning<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    let mut header: TextBuf<20> = TextBuf::new();
    if app.fm_is_channel_mode() {
        write!(header, "FM CH {:02}", app.fm_channel_index() + 1).ok();
    } else {
        write!(header, "FM VFO").ok();
    }
    Text::new(header.as_str(), Point::new(4, 12), small)
        .draw(lcd)
        .ok();

    if app.fm_input_len() > 0 {
        draw_freq_input(lcd, app, 40);
    } else {
        let freq_hz = app.fm_deci_mhz() as u32 * 100_000;
        draw_frequency(lcd, freq_hz, 40);
    }

    let mut status: TextBuf<20> = TextBuf::new();
    if app.fm_is_seeking() {
        write!(status, "SEEK...").ok();
    } else {
        write!(status, "RSSI {}", app.fm_rssi()).ok();
    }
    Text::new(status.as_str(), Point::new(4, 52), small)
        .draw(lcd)
        .ok();

    Text::new("VM CH/VFO  MENU SAVE", Point::new(2, 62), small)
        .draw(lcd)
        .ok();
}

fn draw_freq_input<D>(lcd: &mut D, app: &app::App, baseline_y: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let len = app.fm_input_len();
    let mut buf: TextBuf<8> = TextBuf::new();
    if app.fm_is_channel_mode() {
        for pos in 0..2usize {
            if pos < len {
                write!(buf, "{}", app.fm_input_digit(pos)).ok();
            } else {
                write!(buf, "-").ok();
            }
        }
    } else {
        for pos in 0..4usize {
            if pos == 3 {
                write!(buf, ".").ok();
            }
            if pos < len {
                write!(buf, "{}", app.fm_input_digit(pos)).ok();
            } else {
                write!(buf, "-").ok();
            }
        }
    }
    draw_right_aligned(lcd, buf.as_str(), &PROFONT_14_POINT, baseline_y);
}
