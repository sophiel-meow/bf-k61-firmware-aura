use super::list::{draw_list, ListSource};
use super::standby;
use super::TextBuf;
use crate::app;
use crate::app::satellite::time;
use crate::app::satellite::{DetailField, SatellitePage};
use crate::flash_map::{SatRecord, MAX_SATELLITES};
use core::fmt::Write as _;
use embedded_graphics::mono_font::{
    ascii::{FONT_5X8, FONT_6X10},
    MonoTextStyle,
};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use profont::PROFONT_14_POINT;

const ROW_BASE: i32 = 8;

struct SatListSource<'a>(&'a app::App);

impl ListSource for SatListSource<'_> {
    fn row_count(&mut self) -> usize {
        app::satellite::SatelliteUi::list_item_count(self.0.sats())
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        if index == 0 {
            let _ = w.write_str("Set Time");
        } else if let Some(sat) = sat_at_index(self.0.sats(), index) {
            write_sat_name(w, &sat.name);
        } else {
            let _ = w.write_str("+ Add Satellite");
        }
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        if index == 0 {
            let (h, m, s) = time::hms_from_wall_secs(self.0.wall_secs());
            if self.0.time_set() {
                let _ = write!(w, "{:02}:{:02}:{:02}", h, m, s);
            } else {
                let _ = write!(w, "--:--:--");
            }
            return true;
        }
        if let Some(sat) = sat_at_index(self.0.sats(), index) {
            write_freq_mhz(w, sat.rx_freq_hz);
            return true;
        }
        false
    }
}

fn sat_at_index(sats: &[Option<SatRecord>; MAX_SATELLITES], index: usize) -> Option<&SatRecord> {
    if index == 0 {
        return None; // "Set Time" header
    }
    let sat_pos = index - 1;
    let mut found = 0;
    for slot in sats.iter() {
        if slot.is_some() {
            if found == sat_pos {
                return slot.as_ref();
            }
            found += 1;
        }
    }
    None // "+ Add Satellite" or out of range
}

struct SatDetailSource<'a> {
    ui: &'a app::satellite::SatelliteUi,
    sat: &'a SatRecord,
    is_new: bool,
    time_set: bool,
}

impl ListSource for SatDetailSource<'_> {
    fn row_count(&mut self) -> usize {
        DetailField::count(self.is_new)
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        let field = DetailField::field_at(index, self.is_new);
        let label = match field {
            DetailField::Name => "Name",
            DetailField::RxFreq => "Rx Freq",
            DetailField::TxFreq => "Tx Freq",
            DetailField::RxTone => "Rx Tone",
            DetailField::TxTone => "Tx Tone",
            DetailField::Altitude => "Altitude",
            DetailField::Aos => "AOS",
            DetailField::Los => "LOS",
            DetailField::MaxEl => "Max El",
            DetailField::StartTracking => {
                if self.time_set && self.ui.pass_configured {
                    "Start Tracking"
                } else if !self.time_set {
                    "Set time first!"
                } else {
                    "Set AOS/LOS!"
                }
            }
            DetailField::Save => "Save",
            DetailField::Delete => "Delete",
        };
        let _ = w.write_str(label);
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        let field = DetailField::field_at(index, self.is_new);
        // Save and StartTracking have no value display
        if matches!(field, DetailField::Save | DetailField::StartTracking) {
            return false;
        }
        if field == DetailField::Delete {
            if self.ui.editing {
                let _ = w.write_str("Sure? MENU");
                return true;
            }
            return false;
        }

        let editing = self.ui.editing && self.ui.detail_field == field;

        match field {
            DetailField::Name => {
                if editing {
                    crate::app::name_edit::write_name_plain(&self.ui.name_edit.buf, w);
                } else {
                    write_sat_name(w, &self.sat.name);
                }
                true
            }
            DetailField::RxFreq => {
                if editing {
                    // 6-digit XXX.XXX MHz display (3 int + 3 frac)
                    self.ui.freq_edit.write_display(3, w);
                } else {
                    write_freq_mhz(w, self.sat.rx_freq_hz);
                }
                true
            }
            DetailField::TxFreq => {
                if editing {
                    self.ui.freq_edit.write_display(3, w);
                } else if self.sat.tx_freq_hz == 0 {
                    let _ = w.write_str("OFF");
                } else {
                    write_freq_mhz(w, self.sat.tx_freq_hz);
                }
                true
            }
            DetailField::RxTone => {
                if editing {
                    write_tone_idx(w, self.ui.tone_idx);
                } else {
                    write_tone(w, self.sat.rx_tone_hz);
                }
                true
            }
            DetailField::TxTone => {
                if editing {
                    write_tone_idx(w, self.ui.tone_idx);
                } else {
                    write_tone(w, self.sat.tx_tone_hz);
                }
                true
            }
            DetailField::Altitude => {
                if editing {
                    write_digits_raw(w, &self.ui.alt_edit.digits, self.ui.alt_edit.len, " km");
                } else {
                    let _ = write!(w, "{} km", self.sat.altitude_km);
                }
                true
            }
            DetailField::Aos => {
                if editing {
                    write_time_raw(w, &self.ui.time_edit.digits, self.ui.time_edit.len);
                } else {
                    let _ = write!(
                        w,
                        "{:02}:{:02}:{:02}",
                        self.ui.aos_h, self.ui.aos_m, self.ui.aos_s
                    );
                }
                true
            }
            DetailField::Los => {
                if editing {
                    write_time_raw(w, &self.ui.time_edit.digits, self.ui.time_edit.len);
                } else {
                    let _ = write!(
                        w,
                        "{:02}:{:02}:{:02}",
                        self.ui.los_h, self.ui.los_m, self.ui.los_s
                    );
                }
                true
            }
            DetailField::MaxEl => {
                if editing {
                    write_digits_raw(w, &self.ui.el_edit.digits, self.ui.el_edit.len, " deg");
                } else {
                    let _ = write!(w, "{} deg", self.ui.max_el_deg);
                }
                true
            }
            _ => false,
        }
    }

    fn cursor(&mut self, index: usize) -> Option<usize> {
        if !self.ui.editing {
            return None;
        }
        let field = DetailField::field_at(index, self.is_new);
        if field != self.ui.detail_field {
            return None;
        }
        match field {
            DetailField::Name => Some(self.ui.name_edit.cursor),
            _ => None,
        }
    }
}

pub fn draw_satellite<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let ui = app.satellite_ui();
    let sats = app.sats();
    let time_set = app.time_set();

    match ui.page {
        SatellitePage::List => {
            let selected = ui.list_index;
            let mut source = SatListSource(app);
            draw_list(lcd, "SATELLITE", &mut source, selected, false);

            if ui.confirm_delete {
                draw_confirm_overlay(lcd);
            }
        }
        SatellitePage::Detail => {
            let sat = sats[ui.detail_index].unwrap_or(SatRecord::BLANK);
            let is_new = ui.is_new;

            let mut title = TextBuf::<16>::new();
            if is_new {
                let _ = title.write_str("NEW SAT");
            } else {
                write_sat_name(&mut title, &sat.name);
            }

            let selected = ui.detail_field.field_index(is_new);
            let show_arrows =
                ui.editing && matches!(ui.detail_field, DetailField::RxTone | DetailField::TxTone);
            let mut source = SatDetailSource {
                ui,
                sat: &sat,
                is_new,
                time_set,
            };
            draw_list(lcd, title.as_str(), &mut source, selected, show_arrows);
        }
        SatellitePage::TimeSet => draw_time_set(lcd, ui),
        SatellitePage::Tracking => draw_tracking(lcd, ui, app),
    }
}

fn draw_time_set<D>(lcd: &mut D, ui: &app::satellite::SatelliteUi)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let font6 = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let font6_inv = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
    let font5 = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);

    // Clear
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    draw_centered_6x10(lcd, "SET TIME", 8);

    let time_base_y = 24i32;

    let hh = ui.time_h.value();
    let mm = ui.time_m.value();
    let ss = ui.time_s.value();

    let x_hh: i32 = 40;
    let x_colon1: i32 = 52;
    let x_mm: i32 = 58;
    let x_colon2: i32 = 70;
    let x_ss: i32 = 76;

    let digits = [
        (x_hh, hh, ui.time_field == 0),
        (x_mm, mm, ui.time_field == 1),
        (x_ss, ss, ui.time_field == 2),
    ];

    for &(x, val, active) in &digits {
        if active {
            Rectangle::new(Point::new(x - 1, time_base_y - ROW_BASE), Size::new(13, 12))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(lcd)
                .ok();
        }
        let s = if active { font6_inv } else { font6 };
        let mut buf = TextBuf::<4>::new();
        let _ = write!(buf, "{:02}", val);
        Text::new(buf.as_str(), Point::new(x, time_base_y), s)
            .draw(lcd)
            .ok();
    }

    Text::new(":", Point::new(x_colon1, time_base_y), font6)
        .draw(lcd)
        .ok();
    Text::new(":", Point::new(x_colon2, time_base_y), font6)
        .draw(lcd)
        .ok();

    let hint1_base = 48i32;
    let hint2_base = 58i32;

    Text::new("EXIT=Del  UP/DN=Field", Point::new(0, hint1_base), font5)
        .draw(lcd)
        .ok();

    Text::new("MENU=OK  LongEXIT=Cancel", Point::new(0, hint2_base), font5)
        .draw(lcd)
        .ok();
}

fn draw_tracking<D>(lcd: &mut D, ui: &app::satellite::SatelliteUi, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let font5 = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);
    let font5_inv = MonoTextStyle::new(&FONT_5X8, BinaryColor::Off);
    let font6 = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    // Clear
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let sat_name = app.sats()[ui.detail_index]
        .as_ref()
        .map(|s| &s.name)
        .unwrap_or(&[0; 10]);

    let rx_corrected = (ui.active_rx_freq_hz as i32 + ui.current_doppler_hz) as u32;

    let mut status: TextBuf<32> = TextBuf::new();
    write_sat_name(&mut status, sat_name);
    let wide_str = if ui.tracking_wide { "WIDE" } else { "NARR" };
    // Draw name left-aligned
    Text::new(status.as_str(), Point::new(0, 7), font5)
        .draw(lcd)
        .ok();

    let mut right: TextBuf<20> = TextBuf::new();
    if ui.tracking_monitor {
        let _ = write!(right, "{}  SQL:OFF", wide_str);
    } else {
        let _ = write!(right, "{}  SQL:{}", wide_str, ui.tracking_sql);
    }
    let right_x = 128 - right.as_str().len() as i32 * 5;
    Text::new(right.as_str(), Point::new(right_x, 7), font5)
        .draw(lcd)
        .ok();

    let mhz = rx_corrected / 1_000_000;
    let frac = (rx_corrected % 1_000_000) / 10; // 5 digits
    let khz = frac / 100;
    let tail = frac % 100;

    let mut big: TextBuf<8> = TextBuf::new();
    write!(big, "{:3}.{:03}", mhz, khz).ok();
    let mut small: TextBuf<4> = TextBuf::new();
    write!(small, "{:02}", tail).ok();

    let profont = MonoTextStyle::new(&PROFONT_14_POINT, BinaryColor::On);
    let big_w = big.as_str().len() as i32 * PROFONT_14_POINT.character_size.width as i32;
    let small_w = small.as_str().len() as i32 * FONT_6X10.character_size.width as i32;
    let total_w = big_w + small_w;
    let freq_x = (128 - total_w) / 2;
    let freq_baseline = 8 + PROFONT_14_POINT.baseline as i32;

    Text::new(big.as_str(), Point::new(freq_x, freq_baseline), profont)
        .draw(lcd)
        .ok();
    Text::new(
        small.as_str(),
        Point::new(
            freq_x + big_w,
            freq_baseline - PROFONT_14_POINT.baseline as i32 + FONT_6X10.baseline as i32,
        ),
        font6,
    )
    .draw(lcd)
    .ok();

    let smeter_y = 24i32;
    standby::draw_s_meter(lcd, app, smeter_y);

    let gain_y = 32i32;
    let gain_base = gain_y + FONT_5X8.baseline as i32;
    let gain_names = ["LNAs", "LNA", "PGA", "IF"];
    let gain_vals: [u16; 4] = [
        ui.gain_lnas as u16,
        ui.gain_lna as u16,
        ui.gain_pga as u16,
        ui.gain_if,
    ];
    let gain_widths: [i32; 4] = [32, 27, 27, 40]; // column widths
    let mut gx = 2i32;

    for i in 0..4 {
        let gw = gain_widths[i];
        let selected = ui.gain_menu == (i + 1) as u8; // menu 1..4

        // Highlight background for selected gain
        if selected {
            Rectangle::new(Point::new(gx, gain_y), Size::new(gw as u32, 8))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(lcd)
                .ok();
        }

        let style = if selected { font5_inv } else { font5 };
        let mut label: TextBuf<16> = TextBuf::new();
        let _ = write!(label, "{}:{}", gain_names[i], gain_vals[i]);
        Text::new(label.as_str(), Point::new(gx + 1, gain_base), style)
            .draw(lcd)
            .ok();

        gx += gw;
    }

    let info_y = 40i32;
    let info_base = info_y + FONT_5X8.baseline as i32;

    // Left: TX (uplink) frequency with Doppler correction
    let tx_corrected = if ui.active_tx_freq_hz > 0 && ui.active_rx_freq_hz > 0 {
        let doppler_tx = app::satellite::tracking::scale_doppler_to_tx(
            ui.current_doppler_hz,
            ui.active_rx_freq_hz,
            ui.active_tx_freq_hz,
        );
        (ui.active_tx_freq_hz as i32 + doppler_tx) as u32
    } else {
        0
    };
    let mut freq_str: TextBuf<16> = TextBuf::new();
    if tx_corrected > 0 {
        let _ = write!(freq_str, "TX:");
        write_freq_mhz(&mut freq_str, tx_corrected);
    } else {
        let _ = freq_str.write_str("RX only");
    }
    Text::new(freq_str.as_str(), Point::new(0, info_base), font5)
        .draw(lcd)
        .ok();

    // Right: pass countdown/status
    let now = app.wall_secs();
    let aos = ui.pass_aos_abs;
    let dur = ui.pass_dur_secs;
    let los = aos.saturating_add(dur);

    let mut pass: TextBuf<14> = TextBuf::new();
    if now < aos {
        let remaining = aos - now;
        if remaining > 9999 {
            let _ = pass.write_str("Long");
        } else {
            let _ = write!(pass, "-{}s", remaining);
        }
    } else if now < los {
        let remaining = los - now;
        let _ = write!(pass, "-{}s", remaining);
    } else {
        let _ = pass.write_str("Passed");
    }
    let pass_x = 128 - pass.as_str().len() as i32 * 5;
    Text::new(pass.as_str(), Point::new(pass_x, info_base), font5)
        .draw(lcd)
        .ok();

    let bar_y = 48i32;
    let bar_w = 124i32;
    let bar_h = 6i32;
    let bar_x = 2i32;

    // Background outline
    Rectangle::new(
        Point::new(bar_x, bar_y),
        Size::new(bar_w as u32, bar_h as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(lcd)
    .ok();

    // Filled portion
    if dur > 0 {
        let fraction: u64 = if now < aos {
            // Pre-pass: fill based on countdown remaining (<1000s shown)
            let remaining = (aos - now).min(1000);
            (1000 - remaining) as u64 * bar_w as u64 / 1000
        } else if now < los {
            // During pass: fill based on elapsed
            let elapsed = now - aos;
            elapsed as u64 * bar_w as u64 / dur as u64
        } else {
            // Pass complete: full bar
            bar_w as u64
        };
        let fill_w = (fraction as i32).min(bar_w).max(0);
        if fill_w > 0 {
            Rectangle::new(
                Point::new(bar_x + 1, bar_y + 1),
                Size::new((fill_w - 2).max(1) as u32, (bar_h - 2) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(lcd)
            .ok();
        }
    }

    let hint_y = 56i32;
    let hint_base = hint_y + FONT_5X8.baseline as i32;
    let hint = if ui.gain_menu > 0 {
        "U/D:Adj MENU:Next"
    } else {
        "U/D:SQL MENU:Gain BND:W/N"
    };
    Text::new(hint, Point::new(0, hint_base), font5)
        .draw(lcd)
        .ok();
}

fn write_sat_name(w: &mut dyn core::fmt::Write, name: &[u8; 10]) {
    let end = name
        .iter()
        .position(|&b| b == 0 || b == 0xFF)
        .unwrap_or(name.len());
    if end == 0 {
        let _ = w.write_str("New Sat");
    } else {
        for &b in &name[..end] {
            let _ = w.write_char(b as char);
        }
    }
}

fn write_freq_mhz(w: &mut dyn core::fmt::Write, freq_hz: u32) {
    let mhz = freq_hz / 1_000_000;
    let frac = (freq_hz % 1_000_000) / 1000;
    let _ = write!(w, "{}.{:03}", mhz, frac);
}

fn write_tone(w: &mut dyn core::fmt::Write, tenth_hz: u16) {
    if tenth_hz == 0 {
        let _ = w.write_str("OFF");
    } else {
        let _ = write!(w, "{}.{}Hz", tenth_hz / 10, tenth_hz % 10);
    }
}

/// Write a tone name from index (-1 = OFF, 0..49 = CTCSS_TABLE).
fn write_tone_idx(w: &mut dyn core::fmt::Write, idx: i16) {
    if idx < 0 {
        let _ = w.write_str("OFF");
    } else {
        let hz = app::CTCSS_TABLE[idx as usize];
        let _ = write!(w, "{}.{}Hz", hz / 10, hz % 10);
    }
}

fn write_digits_raw(w: &mut dyn core::fmt::Write, digits: &[u8], len: usize, suffix: &str) {
    if len == 0 {
        let _ = write!(w, "_{}", suffix);
    } else {
        for d in digits.iter().take(len) {
            let _ = write!(w, "{}", d);
        }
        let _ = w.write_str(suffix);
    }
}

fn write_time_raw(w: &mut dyn core::fmt::Write, digits: &[u8; 6], len: usize) {
    let d = |i: usize| -> char {
        if i < len {
            (digits[i] + b'0') as char
        } else {
            '_'
        }
    };
    let _ = write!(w, "{}{}:{}{}:{}{}", d(0), d(1), d(2), d(3), d(4), d(5));
}

fn draw_centered_6x10<D>(lcd: &mut D, text: &str, baseline_y: i32)
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

fn draw_confirm_overlay<D>(lcd: &mut D)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let x = 8;
    let y = 20;
    let w = 112u32;
    let h = 24u32;

    // Background
    Rectangle::new(Point::new(x, y), Size::new(w, h))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(lcd)
        .ok();

    let style = MonoTextStyle::new(&FONT_5X8, BinaryColor::Off);
    Text::new("Delete satellite?", Point::new(x + 4, y + 8), style)
        .draw(lcd)
        .ok();
    Text::new("UP=No  DOWN=Yes", Point::new(x + 4, y + 18), style)
        .draw(lcd)
        .ok();
}
