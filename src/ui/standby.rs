use super::TextBuf;
use crate::app::{self, ChannelDisplayMode};
use crate::device::radio::{Modulation, Power, SubAudio};
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

const STATUS_HEIGHT: i32 = 8;
const BAND_HEIGHT: i32 = 28;
const RIGHT_MARGIN: i32 = 2;

pub fn draw_standby<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    draw_status_bar(lcd, app);
    for i in 0..2usize {
        draw_vfo_band(lcd, app, i);
    }
    if app.tx_prohibited() {
        draw_tx_prohibited_overlay(lcd);
    } else {
        draw_ani_overlay(lcd, app);
    }
}

fn draw_tx_prohibited_overlay<D>(lcd: &mut D)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(16, 28), Size::new(96, 14))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(lcd)
        .ok();
    Text::new(
        "TX PROHIBITED",
        Point::new(25, 38),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off),
    )
    .draw(lcd)
    .ok();
}

fn draw_status_bar<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let small = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);

    if app.is_transmitting() {
        let secs = app.tx_elapsed_seconds();
        let mut buf: TextBuf<8> = TextBuf::new();
        write!(buf, "{:02}:{:02}", secs / 60, secs % 60).ok();
        Text::new(buf.as_str(), Point::new(0, FONT_5X8.baseline as i32), small)
            .draw(lcd)
            .ok();
    } else {
        // TODO: power saving
        if app.power_save_active() {
            Text::new("PS", Point::new(0, FONT_5X8.baseline as i32), small)
                .draw(lcd)
                .ok();
        }
        if app.dual_standby_enabled() {
            Text::new("DWR", Point::new(16, FONT_5X8.baseline as i32), small)
                .draw(lcd)
                .ok();
        }
    }

    if app.vox_enabled() {
        Text::new("VX", Point::new(84, FONT_5X8.baseline as i32), small)
            .draw(lcd)
            .ok();
    }
    if app.is_key_locked() {
        let lock = super::icons::key_lock();
        Image::new(&lock, Point::new(99, 0)).draw(lcd).ok();
    }
    super::icons::draw_battery(lcd, 128 - RIGHT_MARGIN, 0, app.battery_bars() + 2);
}

fn draw_vfo_band<D>(lcd: &mut D, app: &app::App, i: usize)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let top = STATUS_HEIGHT + i as i32 * BAND_HEIGHT;
    let is_master = i == app.master_index();
    let is_watching = i == app.watching_index();
    let transmitting_here = app.is_transmitting() && is_master;
    let small = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);

    // Row A: master marker (master side only) + TX / RX / dual-watch ">>"
    if is_master {
        let marker = super::icons::vfo_default();
        Image::new(&marker, Point::new(2, top)).draw(lcd).ok();
    }
    let status = if transmitting_here {
        "TX"
    } else if is_watching && app.audio_open() {
        "RX"
    } else if app.last_signal_side() == Some(i) {
        ">>"
    } else {
        ""
    };
    Text::new(
        status,
        Point::new(12, top + FONT_5X8.baseline as i32),
        small,
    )
    .draw(lcd)
    .ok();

    // Row B: channel number / VFO label
    let mut chan_label: TextBuf<8> = TextBuf::new();
    if app.side_is_channel_mode(i) {
        write!(chan_label, "M{}", app.side_channel_num(i)).ok();
    } else {
        write!(chan_label, "VFO").ok();
    }
    Text::new(
        chan_label.as_str(),
        Point::new(2, top + 8 + FONT_5X8.baseline as i32),
        small,
    )
    .draw(lcd)
    .ok();

    // Right-hand primary slot
    let channel_mode = app.side_is_channel_mode(i);
    let name = app.side_name_str(i);
    let has_name = channel_mode && !name.is_empty();

    let freq_hz = if transmitting_here {
        app.side_tx_freq_hz(i)
    } else {
        app.side_freq_hz(i)
    };

    if is_master && app.freq_input_len() > 0 {
        // User is mid-entry on a VFO frequency: show "xxx.xxx" with dashes
        // for not-yet-typed digits.
        // this overrides whatever ChannelDisplayMode is currently active.
        draw_freq_input(lcd, app, top + PROFONT_14_POINT.baseline as i32);
    } else {
        match app.channel_display_mode() {
            ChannelDisplayMode::NameFreq if has_name => {
                let shown = &name[..name.len().min(12)];
                let name_y = top + FONT_6X10.baseline as i32;
                draw_right_aligned(lcd, shown, &FONT_6X10, name_y);

                let mhz = freq_hz / 1_000_000;
                let frac = (freq_hz % 1_000_000) / 10;
                let mut freq_line: TextBuf<12> = TextBuf::new();
                write!(freq_line, "{:3}.{:05}", mhz, frac).ok();
                draw_right_aligned(
                    lcd,
                    freq_line.as_str(),
                    &FONT_6X10,
                    name_y + FONT_6X10.character_size.height as i32,
                );
            }
            ChannelDisplayMode::Name if has_name => {
                let shown = &name[..name.len().min(8)];
                draw_right_aligned(
                    lcd,
                    shown,
                    &PROFONT_14_POINT,
                    top + PROFONT_14_POINT.baseline as i32,
                );
            }
            _ => {
                draw_frequency(lcd, freq_hz, top + PROFONT_14_POINT.baseline as i32);
            }
        }
    }

    // Row C: mode/tone/power/direction line, or TX antenna+power-bars
    if transmitting_here {
        draw_tx_power_bars(lcd, app, top);
    } else {
        draw_mode_line(lcd, app, i, top + BAND_HEIGHT - 1);
    }
}

fn draw_tx_power_bars<D>(lcd: &mut D, app: &app::App, top: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let y = top + BAND_HEIGHT - 8;
    let ant = super::icons::antenna();
    Image::new(&ant, Point::new(2, y)).draw(lcd).ok();

    let level = match app.side_power(app.master_index()) {
        Power::Low => 2,
        Power::Mid => 4,
        Power::High => 6,
    };
    for k in 1..=level {
        let h = (k as i32 + 1).min(8);
        Rectangle::new(
            Point::new(2 + 2 + k as i32 * 3, y + 8 - h),
            Size::new(2, h as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(lcd)
        .ok();
    }
}

pub(super) fn draw_right_aligned<D>(
    lcd: &mut D,
    text: &str,
    font: &embedded_graphics::mono_font::MonoFont,
    y: i32,
) where
    D: DrawTarget<Color = BinaryColor>,
{
    let width = text.chars().count() as i32 * font.character_size.width as i32;
    let x = 128 - RIGHT_MARGIN - width;
    Text::new(
        text,
        Point::new(x, y),
        MonoTextStyle::new(font, BinaryColor::On),
    )
    .draw(lcd)
    .ok();
}

/// Big MHz.kHz frequency (ProFont) with the remaining 2 fractional digits
/// trailing in a smaller font, both right-aligned as one block.
pub(super) fn draw_frequency<D>(lcd: &mut D, freq_hz: u32, baseline_y: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mhz = freq_hz / 1_000_000;
    let frac = (freq_hz % 1_000_000) / 10; // 5 fractional digits (kHz + 2 extra)
    let khz = frac / 100;
    let tail = frac % 100;

    let mut big: TextBuf<8> = TextBuf::new();
    write!(big, "{:3}.{:03}", mhz, khz).ok();
    let mut small: TextBuf<4> = TextBuf::new();
    write!(small, "{:02}", tail).ok();

    let big_width = big.as_str().len() as i32 * PROFONT_14_POINT.character_size.width as i32;
    let small_width = small.as_str().len() as i32 * FONT_6X10.character_size.width as i32;
    let big_x = 128 - RIGHT_MARGIN - small_width - big_width;

    Text::new(
        big.as_str(),
        Point::new(big_x, baseline_y),
        MonoTextStyle::new(&PROFONT_14_POINT, BinaryColor::On),
    )
    .draw(lcd)
    .ok();
    Text::new(
        small.as_str(),
        Point::new(
            big_x + big_width,
            baseline_y - PROFONT_14_POINT.baseline as i32 + FONT_6X10.baseline as i32,
        ),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
    )
    .draw(lcd)
    .ok();
}

/// Mid-entry VFO frequency: `"xxx.x--"`
fn draw_freq_input<D>(lcd: &mut D, app: &app::App, baseline_y: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let len = app.freq_input_len();
    let mut buf: TextBuf<8> = TextBuf::new();
    for pos in 0..6usize {
        if pos == 3 {
            write!(buf, ".").ok();
        }
        if pos < len {
            write!(buf, "{}", app.freq_input_digit(pos)).ok();
        } else {
            write!(buf, "-").ok();
        }
    }
    draw_right_aligned(lcd, buf.as_str(), &PROFONT_14_POINT, baseline_y);
}

fn tone_mode_label(tx: SubAudio, rx: SubAudio) -> &'static str {
    match (tx, rx) {
        (SubAudio::None, SubAudio::None) => "",
        (SubAudio::Ctcss(_), SubAudio::None) => "Tone",
        (SubAudio::Ctcss(t), SubAudio::Ctcss(r)) if t == r => "TSQL",
        (
            SubAudio::Dcs {
                code: tc,
                inverted: ti,
            },
            SubAudio::Dcs {
                code: rc,
                inverted: ri,
            },
        ) if tc == rc && ti == ri => "DTCS",
        (SubAudio::None, SubAudio::Dcs { .. }) => "DTCS-R",
        (SubAudio::None, SubAudio::Ctcss(_)) => "TSQL-R",
        _ => "Cross",
    }
}

/// Info line: modulation, TX power, subaudio tone, and
/// repeater shift direction.
fn draw_mode_line<D>(lcd: &mut D, app: &app::App, i: usize, baseline_y: i32)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let modulation = match app.side_modulation(i) {
        Modulation::Fm => "FM",
        Modulation::Am => "AM",
    };
    let power = match app.side_power(i) {
        Power::Low => "Lo",
        Power::Mid => "Md",
        Power::High => "Hi",
    };
    let tone = tone_mode_label(app.side_subaudio_tx(i), app.side_subaudio_rx(i));
    let dir = if app.side_offset_hz(i) == 0 {
        ""
    } else {
        match app.side_freq_dir(i) {
            1 => "+",
            2 => "-",
            _ => "",
        }
    };

    let mut line: TextBuf<20> = TextBuf::new();
    write!(line, "{} {} {} {}", modulation, power, tone, dir).ok();
    Text::new(
        line.as_str(),
        Point::new(2, baseline_y),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
    )
    .draw(lcd)
    .ok();
}

fn dtmf_char(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        10..=13 => (b'A' + (v - 10)) as char,
        14 => '*',
        15 => '#',
        _ => '?',
    }
}

fn draw_ani_overlay<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let Some(caller) = app.ani_caller_id() else {
        return;
    };

    let mut line: TextBuf<12> = TextBuf::new();
    write!(
        line,
        "ANI {}{}{}",
        dtmf_char(caller[0]),
        dtmf_char(caller[1]),
        dtmf_char(caller[2])
    )
    .ok();

    Rectangle::new(Point::new(16, 28), Size::new(96, 14))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(lcd)
        .ok();
    Text::new(
        line.as_str(),
        Point::new(24, 38),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::Off),
    )
    .draw(lcd)
    .ok();
}
