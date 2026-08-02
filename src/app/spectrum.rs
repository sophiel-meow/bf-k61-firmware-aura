//! Spectrum-analyzer app: fast RSSI-vs-frequency sweep with a live bar
//! graph.
//!
//! The scan/UI design here is adapted from the spectrum app in the
//! UV-K5 firmware (https://github.com/egzumer/uv-k5-firmware-custom, `app/
//! spectrum.c`; the feature itself originates from fagci's spectrum mod,
//! merged in via Egzumer's firmware.
//! That project is Apache License 2.0; reused here under its terms.

use super::{App, Mode, VFO_INPUT_DIGITS};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::device::radio::Modulation;
use cortex_m::peripheral::SYST;

const MAX_BINS: usize = 128;

/// Selectable scan step sizes, Hz
const SCAN_STEPS_HZ: [u32; 15] = [
    10, 100, 500, 1_000, 2_500, 5_000, 6_250, 8_330, 10_000, 12_500, 15_000, 20_000, 25_000,
    50_000, 100_000,
];
const DEFAULT_SCAN_STEP_INDEX: u8 = 8; // 10kHz

/// Bins actually swept
const STEPS_COUNT_TABLE: [u16; 4] = [128, 64, 32, 16];
const DEFAULT_STEPS_INDEX: u8 = 1; // 64 bins, 2px/bar

const PAN_STEP_MIN_HZ: u32 = 100_000;
const PAN_STEP_MAX_HZ: u32 = 2_000_000;
const DEFAULT_PAN_STEP_HZ: u32 = 800_000;

const RSSI_CEILING_MIN: i16 = -120;
const RSSI_CEILING_MAX: i16 = -20;
const RSSI_CEILING_STEP: i16 = 4;
const DEFAULT_RSSI_CEILING: i16 = -60;

const TRIGGER_STEP: u16 = 4;
const DEFAULT_TRIGGER_DBM: i16 = -90;

fn rssi_raw_for_dbm(dbm: i16) -> u16 {
    super::dbm_to_rssi_raw(dbm).clamp(0, u16::MAX as i32) as u16
}

fn max_trigger_for_ceiling(ceiling_dbm: i16) -> u16 {
    rssi_raw_for_dbm(ceiling_dbm)
}

const BLACKLIST_SENTINEL: u16 = u16::MAX;

const PEAK_HOLD_STEPS: u16 = 1024;

const LISTEN_HANG_TICKS: u16 = 30;

const STEPS_PER_TICK_WIDE: u8 = 2;
const STEPS_PER_TICK_NARROW: u8 = 1;

pub(super) struct SpectrumState {
    /// Frequency of bin 0.
    window_start_hz: u32,
    scan_step_index: u8,
    pan_step_hz: u32,
    steps_index: u8,
    /// Bin currently being (or about to be) measured.
    scan_pos: u16,
    rssi_bins: [u16; MAX_BINS],
    rssi_ceiling: i16,
    trigger_level: u16,
    peak_bin: Option<u16>,
    peak_rssi: u16,
    peak_age: u16,
    listening: bool,
    listen_hang: u16,
    wide_bandwidth: bool,
    modulation: Modulation,
    /// `true` while the "5" frequency-entry overlay is open
    entering_freq: bool,
}

impl SpectrumState {
    pub(super) fn new(centre_hz: u32) -> Self {
        let mut s = SpectrumState {
            window_start_hz: 0,
            scan_step_index: DEFAULT_SCAN_STEP_INDEX,
            pan_step_hz: DEFAULT_PAN_STEP_HZ,
            steps_index: DEFAULT_STEPS_INDEX,
            scan_pos: 0,
            rssi_bins: [0; MAX_BINS],
            rssi_ceiling: DEFAULT_RSSI_CEILING,
            trigger_level: rssi_raw_for_dbm(DEFAULT_TRIGGER_DBM)
                .min(max_trigger_for_ceiling(DEFAULT_RSSI_CEILING)),
            peak_bin: None,
            peak_rssi: 0,
            peak_age: 0,
            listening: false,
            listen_hang: 0,
            wide_bandwidth: true,
            modulation: Modulation::Fm,
            entering_freq: false,
        };
        s.centre_window(centre_hz);
        s
    }

    fn scan_step_hz(&self) -> u32 {
        SCAN_STEPS_HZ[self.scan_step_index as usize]
    }

    fn bins(&self) -> u16 {
        STEPS_COUNT_TABLE[self.steps_index as usize]
    }

    fn centre_window(&mut self, centre_hz: u32) {
        let half_span = (self.bins() as u32 / 2) * self.scan_step_hz();
        self.window_start_hz = centre_hz.saturating_sub(half_span);
    }

    fn freq_at(&self, pos: u16) -> u32 {
        self.window_start_hz + pos as u32 * self.scan_step_hz()
    }

    fn relaunch(&mut self) {
        self.rssi_bins = [0; MAX_BINS];
        self.scan_pos = 0;
        self.peak_bin = None;
        self.peak_rssi = 0;
        self.peak_age = 0;
    }
}

pub(super) fn enter(app: &mut App, syst: &mut SYST) {
    let centre_hz = app.master_freq_hz();
    app.spectrum = SpectrumState::new(centre_hz);
    app.mode = Mode::Spectrum;
    app.input.clear();
    app.sync_watching_to_master(syst);
    let wide = app.spectrum.wide_bandwidth;
    app.radio.spectrum_set_wide(syst, wide);
}

fn exit(app: &mut App, syst: &mut SYST) {
    app.spectrum.listening = false;
    app.mode = Mode::Standby;
    app.reset_key_idle();
    app.sync_watching_to_master(syst);
}

pub(super) fn poll(app: &mut App, syst: &mut SYST) {
    if app.mode != Mode::Spectrum || app.spectrum.entering_freq {
        return;
    }
    if app.spectrum.listening {
        poll_listen(app, syst);
    } else {
        let steps = if app.spectrum.wide_bandwidth {
            STEPS_PER_TICK_WIDE
        } else {
            STEPS_PER_TICK_NARROW
        };
        for _ in 0..steps {
            step_scan(app, syst);
            if app.spectrum.listening {
                break;
            }
        }
    }
}

fn step_scan(app: &mut App, syst: &mut SYST) {
    let pos = app.spectrum.scan_pos;
    let freq = app.spectrum.freq_at(pos);

    if app.spectrum.rssi_bins[pos as usize] != BLACKLIST_SENTINEL {
        app.radio.spectrum_tune(syst, freq);
        let rssi = app.radio.spectrum_rssi(syst);
        app.spectrum.rssi_bins[pos as usize] = rssi;

        if app.spectrum.peak_bin.is_none()
            || rssi > app.spectrum.peak_rssi
            || app.spectrum.peak_age >= PEAK_HOLD_STEPS
        {
            app.spectrum.peak_bin = Some(pos);
            app.spectrum.peak_rssi = rssi;
            app.spectrum.peak_age = 0;
        } else {
            app.spectrum.peak_age = app.spectrum.peak_age.saturating_add(1);
        }

        if rssi >= app.spectrum.trigger_level {
            start_listening(app, syst, freq);
            return;
        }
    }

    app.spectrum.scan_pos += 1;
    if app.spectrum.scan_pos >= app.spectrum.bins() {
        app.spectrum.scan_pos = 0;
    }
}

fn start_listening(app: &mut App, syst: &mut SYST, freq: u32) {
    app.spectrum.listening = true;
    app.spectrum.listen_hang = 0;
    app.radio.spectrum_tune(syst, freq);
    let (wide, modulation) = (app.spectrum.wide_bandwidth, app.spectrum.modulation);
    app.radio.spectrum_set_modulation(syst, modulation);
    app.radio.spectrum_listen(syst, true, wide, modulation);
}

fn stop_listening(app: &mut App, syst: &mut SYST) {
    app.spectrum.listening = false;
    let (wide, modulation) = (app.spectrum.wide_bandwidth, app.spectrum.modulation);
    app.radio.spectrum_listen(syst, false, wide, modulation);
}

fn poll_listen(app: &mut App, syst: &mut SYST) {
    let pos = app.spectrum.peak_bin.unwrap_or(0);
    let rssi = app.radio.spectrum_rssi(syst);
    app.spectrum.rssi_bins[pos as usize] = rssi;
    if rssi >= app.spectrum.trigger_level {
        app.spectrum.listen_hang = 0;
    } else {
        app.spectrum.listen_hang += 1;
        if app.spectrum.listen_hang >= LISTEN_HANG_TICKS {
            stop_listening(app, syst);
        }
    }
}

pub(super) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if app.spectrum.entering_freq {
        dispatch_freq_input(app, ev);
        return;
    }
    if ev.kind == KeyEventKind::Single && ev.key == KeyId::Exit {
        exit(app, syst);
        return;
    }
    if app.spectrum.listening && ev.kind == KeyEventKind::Single {
        stop_listening(app, syst);
    }
    match (ev.kind, ev.key) {
        (KeyEventKind::Single | KeyEventKind::Repeat, KeyId::Up) => pan(app, true),
        (KeyEventKind::Single | KeyEventKind::Repeat, KeyId::Down) => pan(app, false),
        (KeyEventKind::Single, KeyId::Digit1) => change_scan_step(app, true),
        (KeyEventKind::Single, KeyId::Digit7) => change_scan_step(app, false),
        (KeyEventKind::Single, KeyId::Digit2) => change_pan_step(app, true),
        (KeyEventKind::Single, KeyId::Digit8) => change_pan_step(app, false),
        (KeyEventKind::Single, KeyId::Digit3) => change_ceiling(app, true),
        (KeyEventKind::Single, KeyId::Digit9) => change_ceiling(app, false),
        (KeyEventKind::Single, KeyId::Side1) => blacklist_peak(app),
        (KeyEventKind::Single, KeyId::Asterisk) => change_trigger(app, true),
        (KeyEventKind::Single, KeyId::Pound) => change_trigger(app, false),
        (KeyEventKind::Single, KeyId::Digit5) => {
            app.spectrum.entering_freq = true;
            app.input.clear();
        }
        (KeyEventKind::Single, KeyId::Digit0) => cycle_modulation(app, syst),
        (KeyEventKind::Single, KeyId::Digit6) => toggle_bandwidth(app, syst),
        (KeyEventKind::Single, KeyId::Digit4) => cycle_steps_count(app),
        _ => {}
    }
}

fn pan(app: &mut App, up: bool) {
    let step = app.spectrum.pan_step_hz;
    app.spectrum.window_start_hz = if up {
        app.spectrum.window_start_hz.saturating_add(step)
    } else {
        app.spectrum.window_start_hz.saturating_sub(step)
    };
    app.spectrum.relaunch();
}

fn change_scan_step(app: &mut App, up: bool) {
    let len = SCAN_STEPS_HZ.len() as u8;
    app.spectrum.scan_step_index = if up {
        (app.spectrum.scan_step_index + 1) % len
    } else {
        (app.spectrum.scan_step_index + len - 1) % len
    };
    app.spectrum.relaunch();
}

fn change_pan_step(app: &mut App, up: bool) {
    let s = &mut app.spectrum;
    s.pan_step_hz = if up {
        (s.pan_step_hz * 2).min(PAN_STEP_MAX_HZ)
    } else {
        (s.pan_step_hz / 2).max(PAN_STEP_MIN_HZ)
    };
}

fn change_ceiling(app: &mut App, up: bool) {
    let s = &mut app.spectrum;
    s.rssi_ceiling = if up {
        (s.rssi_ceiling + RSSI_CEILING_STEP).min(RSSI_CEILING_MAX)
    } else {
        (s.rssi_ceiling - RSSI_CEILING_STEP).max(RSSI_CEILING_MIN)
    };
    s.trigger_level = s.trigger_level.min(max_trigger_for_ceiling(s.rssi_ceiling));
}

fn change_trigger(app: &mut App, up: bool) {
    let max = max_trigger_for_ceiling(app.spectrum.rssi_ceiling);
    let s = &mut app.spectrum;
    s.trigger_level = if up {
        (s.trigger_level + TRIGGER_STEP).min(max)
    } else {
        s.trigger_level.saturating_sub(TRIGGER_STEP)
    };
}

fn blacklist_peak(app: &mut App) {
    if let Some(pos) = app.spectrum.peak_bin {
        app.spectrum.rssi_bins[pos as usize] = BLACKLIST_SENTINEL;
        app.spectrum.peak_bin = None;
        app.spectrum.peak_rssi = 0;
    }
}

fn cycle_modulation(app: &mut App, syst: &mut SYST) {
    app.spectrum.modulation = match app.spectrum.modulation {
        Modulation::Fm => Modulation::Am,
        Modulation::Am => Modulation::Usb,
        _ => Modulation::Fm,
    };
    let modulation = app.spectrum.modulation;
    app.radio.spectrum_set_modulation(syst, modulation);
}

fn toggle_bandwidth(app: &mut App, syst: &mut SYST) {
    app.spectrum.wide_bandwidth = !app.spectrum.wide_bandwidth;
    let wide = app.spectrum.wide_bandwidth;
    app.radio.spectrum_set_wide(syst, wide);
}

fn cycle_steps_count(app: &mut App) {
    let len = STEPS_COUNT_TABLE.len() as u8;
    app.spectrum.steps_index = (app.spectrum.steps_index + 1) % len;
    app.spectrum.relaunch();
}

fn dispatch_freq_input(app: &mut App, ev: KeyEvent) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(d) = super::digit_value(ev.key) {
        app.input.push(d);
        if app.input.len >= VFO_INPUT_DIGITS {
            commit_freq_input(app);
        }
        return;
    }
    match ev.key {
        KeyId::Menu => commit_freq_input(app),
        KeyId::Exit => {
            if app.input.len > 0 {
                app.input.clear();
            } else {
                app.spectrum.entering_freq = false;
            }
        }
        _ => {}
    }
}

fn commit_freq_input(app: &mut App) {
    if app.input.len > 0 {
        let centre_hz = app.input.value() * 1000;
        app.spectrum.centre_window(centre_hz);
        app.spectrum.relaunch();
    }
    app.input.clear();
    app.spectrum.entering_freq = false;
}

pub(super) fn window_start_hz(app: &App) -> u32 {
    app.spectrum.window_start_hz
}

pub(super) fn scan_step_hz(app: &App) -> u32 {
    app.spectrum.scan_step_hz()
}

pub(super) fn pan_step_hz(app: &App) -> u32 {
    app.spectrum.pan_step_hz
}

pub(super) fn bins(app: &App) -> u16 {
    app.spectrum.bins()
}

pub(super) fn rssi_bin(app: &App, pos: usize) -> u16 {
    app.spectrum.rssi_bins[pos]
}

pub(super) fn rssi_ceiling(app: &App) -> i16 {
    app.spectrum.rssi_ceiling
}

pub(super) fn trigger_level(app: &App) -> u16 {
    app.spectrum.trigger_level
}

pub(super) fn peak_bin(app: &App) -> Option<u16> {
    app.spectrum.peak_bin
}

pub(super) fn listening(app: &App) -> bool {
    app.spectrum.listening
}

pub(super) fn modulation(app: &App) -> Modulation {
    app.spectrum.modulation
}

pub(super) fn wide_bandwidth(app: &App) -> bool {
    app.spectrum.wide_bandwidth
}

pub(super) fn entering_freq(app: &App) -> bool {
    app.spectrum.entering_freq
}

pub(super) fn input_len(app: &App) -> usize {
    app.input.len
}

pub(super) fn input_digit(app: &App, idx: usize) -> u8 {
    app.input.digits[idx]
}
