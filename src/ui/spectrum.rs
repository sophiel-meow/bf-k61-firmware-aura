//! Spectrum-analyzer bar-graph rendering.
//!
//! The scan/UI design here is adapted from the spectrum app in the
//! UV-K5 firmware (https://github.com/egzumer/uv-k5-firmware-custom, `app/
//! spectrum.c`; the feature itself originates from fagci's spectrum mod,
//! merged in via Egzumer's firmware.
//! That project is Apache License 2.0; reused here under its terms.

use super::TextBuf;
use crate::app;
use crate::device::radio::Modulation;
use core::fmt::Write as _;
use embedded_graphics::mono_font::{
    ascii::{FONT_5X8, FONT_6X10},
    MonoTextStyle,
};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics::Pixel;

const GRAPH_TOP: i32 = 9;
const GRAPH_BOTTOM: i32 = 46;

const DBM_FLOOR: i32 = -141;

fn rssi_to_y(rssi: u16, ceiling_dbm: i16) -> i32 {
    let ceiling_dbm = (ceiling_dbm as i32).max(DBM_FLOOR + 1);
    let dbm = app::rssi_raw_to_dbm(rssi).clamp(DBM_FLOOR, ceiling_dbm);
    let height = GRAPH_BOTTOM - GRAPH_TOP;
    let span = ceiling_dbm - DBM_FLOOR;
    GRAPH_BOTTOM - ((dbm - DBM_FLOOR) * height) / span
}

fn fmt_hz_short(buf: &mut TextBuf<10>, hz: u32) {
    if hz >= 1_000_000 {
        write!(buf, "{}.{}M", hz / 1_000_000, (hz / 100_000) % 10).ok();
    } else if hz >= 1_000 {
        write!(buf, "{}.{}k", hz / 1_000, (hz / 100) % 10).ok();
    } else {
        write!(buf, "{}", hz).ok();
    }
}

fn fmt_mhz(buf: &mut dyn core::fmt::Write, hz: u32) {
    let _ = write!(buf, "{}.{:03}", hz / 1_000_000, (hz / 1_000) % 1_000);
}

pub fn draw_spectrum<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let tiny = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);

    let start_hz = app.spectrum_window_start_hz();
    let bins = app.spectrum_bins();
    let step_hz = app.spectrum_scan_step_hz();
    let end_hz = start_hz + step_hz * bins as u32;
    let ceiling = app.spectrum_rssi_ceiling();

    // window edges
    let mut left: TextBuf<12> = TextBuf::new();
    fmt_mhz(&mut left, start_hz);
    Text::new(left.as_str(), Point::new(0, 8), small)
        .draw(lcd)
        .ok();

    let mut right: TextBuf<12> = TextBuf::new();
    fmt_mhz(&mut right, end_hz);
    let rw = right.as_str().len() as i32 * 6;
    Text::new(right.as_str(), Point::new(128 - rw, 8), small)
        .draw(lcd)
        .ok();

    // bars, one per bin
    let bar_width = (128u16 / bins.max(1)).max(1);
    for i in 0..bins {
        let rssi = app.spectrum_rssi_bin(i as usize);
        if rssi == u16::MAX {
            continue; // blacklisted / not sampled yet this sweep
        }
        let y = rssi_to_y(rssi, ceiling);
        let h = (GRAPH_BOTTOM - y).max(0) as u32;
        Rectangle::new(
            Point::new(i as i32 * bar_width as i32, y),
            Size::new(bar_width as u32, h),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(lcd)
        .ok();
    }

    // trigger level, dashed
    let trig_y = rssi_to_y(app.spectrum_trigger_level(), ceiling);
    let mut x = 0;
    while x < 128 {
        Pixel(Point::new(x, trig_y), BinaryColor::On).draw(lcd).ok();
        x += 3;
    }

    // peak marker: a short tick just above the graph
    if let Some(peak) = app.spectrum_peak_bin() {
        let x = peak as i32 * bar_width as i32;
        for dy in 0..3 {
            Pixel(Point::new(x, GRAPH_TOP - 1 - dy), BinaryColor::On)
                .draw(lcd)
                .ok();
        }
    }

    if app.spectrum_entering_freq() {
        draw_freq_input(lcd, app);
        return;
    }

    let mut status: TextBuf<32> = TextBuf::new();
    let mut step_buf: TextBuf<10> = TextBuf::new();
    fmt_hz_short(&mut step_buf, step_hz);
    let modu = match app.spectrum_modulation() {
        Modulation::Fm => "FM",
        Modulation::Am => "AM",
        Modulation::Usb => "USB",
        _ => "FM",
    };
    let bw = if app.spectrum_wide_bandwidth() {
        "W"
    } else {
        "N"
    };
    write!(
        status,
        "ST{} B{} {}{} TRG{}",
        step_buf.as_str(),
        bins,
        modu,
        bw,
        app.spectrum_trigger_level()
    )
    .ok();
    Text::new(status.as_str(), Point::new(0, 54), tiny)
        .draw(lcd)
        .ok();

    let mut line2: TextBuf<32> = TextBuf::new();
    if app.spectrum_listening() {
        write!(line2, "RX").ok();
    } else if let Some(peak) = app.spectrum_peak_bin() {
        let freq = start_hz + peak as u32 * step_hz;
        write!(line2, "PK ").ok();
        fmt_mhz(&mut line2, freq);
        let dbm = app::rssi_raw_to_dbm(app.spectrum_rssi_bin(peak as usize));
        write!(line2, " {}dBm", dbm).ok();
    } else {
        let mut pan_buf: TextBuf<10> = TextBuf::new();
        fmt_hz_short(&mut pan_buf, app.spectrum_pan_step_hz());
        write!(line2, "PAN {}", pan_buf.as_str()).ok();
    }
    Text::new(line2.as_str(), Point::new(0, 63), tiny)
        .draw(lcd)
        .ok();
}

fn draw_freq_input<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let tiny = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);
    let len = app.spectrum_input_len();
    let mut buf: TextBuf<8> = TextBuf::new();
    for pos in 0..6usize {
        if pos == 3 {
            write!(buf, ".").ok();
        }
        if pos < len {
            write!(buf, "{}", app.spectrum_input_digit(pos)).ok();
        } else {
            write!(buf, "-").ok();
        }
    }
    Text::new(buf.as_str(), Point::new(0, 63), tiny)
        .draw(lcd)
        .ok();
}
