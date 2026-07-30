use super::settings;
use super::{App, ChVfoMode, Mode};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::device::radio::{self, RawTone, SubAudio};
use cortex_m::peripheral::SYST;

const SQUELCH_DEBOUNCE_TICKS: u8 = 5;
const CTCSS_HIT_COUNT: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanQtPhase {
    WaitSquelch,
    Detecting,
    Found,
}

pub(super) struct ScanQtState {
    phase: ScanQtPhase,
    debounce: u8,
    ctcss_candidate: Option<u16>,
    hit_count: u8,
    tone: Option<SubAudio>,
}

impl ScanQtState {
    pub(super) const fn new() -> Self {
        ScanQtState {
            phase: ScanQtPhase::WaitSquelch,
            debounce: 0,
            ctcss_candidate: None,
            hit_count: 0,
            tone: None,
        }
    }
}

pub(super) fn enter(app: &mut App, syst: &mut SYST) {
    app.scanqt = ScanQtState::new();
    app.mode = Mode::ScanQt;
    app.input.clear();
    app.sync_watching_to_master(syst);
}

fn exit(app: &mut App, syst: &mut SYST) {
    app.radio.set_subaudio_scan_filter(syst, false);
    app.mode = Mode::Standby;
    app.reset_key_idle();
    app.sync_watching_to_master(syst);
}

pub(super) fn poll(app: &mut App, syst: &mut SYST) {
    if app.mode != Mode::ScanQt {
        return;
    }
    let squelch_open = app.radio.rssi_open();

    match app.scanqt.phase {
        ScanQtPhase::WaitSquelch => {
            if squelch_open {
                app.scanqt.debounce += 1;
                if app.scanqt.debounce >= SQUELCH_DEBOUNCE_TICKS {
                    app.scanqt.debounce = 0;
                    app.scanqt.hit_count = 0;
                    app.scanqt.ctcss_candidate = None;
                    app.scanqt.phase = ScanQtPhase::Detecting;
                    app.radio.set_subaudio_scan_filter(syst, true);
                }
            } else {
                app.scanqt.debounce = 0;
            }
        }
        ScanQtPhase::Detecting => {
            if !squelch_open {
                app.scanqt.debounce += 1;
                if app.scanqt.debounce >= SQUELCH_DEBOUNCE_TICKS {
                    app.scanqt.debounce = 0;
                    app.scanqt.phase = ScanQtPhase::WaitSquelch;
                    app.radio.set_subaudio_scan_filter(syst, false);
                }
                return;
            }
            app.scanqt.debounce = 0;
            match app.radio.detect_subaudio_raw(syst) {
                // a single successful decode is trusted immediately, no repeat-agreement needed.
                RawTone::Dcs(raw) => {
                    if let Some(code) = radio::find_standard_dcs(raw, &settings::DCS_TABLE) {
                        app.scanqt.tone = Some(SubAudio::Dcs {
                            code,
                            inverted: false,
                        });
                        app.scanqt.phase = ScanQtPhase::Found;
                    }
                }
                RawTone::Ctcss(raw) => {
                    let tenths = radio::ctcss_raw_to_tenths_hz(raw);
                    match radio::find_standard_ctcss(tenths, &settings::CTCSS_TABLE) {
                        Some(hz) if app.scanqt.ctcss_candidate == Some(hz) => {
                            app.scanqt.hit_count += 1;
                            if app.scanqt.hit_count >= CTCSS_HIT_COUNT {
                                app.scanqt.tone = Some(SubAudio::Ctcss(hz));
                                app.scanqt.phase = ScanQtPhase::Found;
                            }
                        }
                        Some(hz) => {
                            app.scanqt.ctcss_candidate = Some(hz);
                            app.scanqt.hit_count = 1;
                        }
                        None => {
                            app.scanqt.ctcss_candidate = None;
                            app.scanqt.hit_count = 0;
                        }
                    }
                }
                RawTone::None => {
                    app.scanqt.ctcss_candidate = None;
                    app.scanqt.hit_count = 0;
                }
            }
        }
        ScanQtPhase::Found => {}
    }
}

fn save(app: &mut App, syst: &mut SYST) {
    if let Some(tone) = app.scanqt.tone {
        let s = &mut app.sides[app.master];
        match tone {
            // CTCSS writes both RX and TX, standard DCS only writes RX.
            SubAudio::Ctcss(_) => {
                s.cfg.subaudio_rx = tone;
                s.cfg.subaudio_tx = tone;
            }
            SubAudio::Dcs { .. } => s.cfg.subaudio_rx = tone,
            SubAudio::None => {}
        }
        if s.vfo_chan == ChVfoMode::Vfo {
            app.save_vfo();
        }
        // Channel mode: skip
    }
    exit(app, syst);
}

pub(super) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    match ev.key {
        KeyId::Up | KeyId::Down if app.scanqt.phase == ScanQtPhase::Found => {
            app.scanqt.tone = None;
            app.scanqt.phase = ScanQtPhase::WaitSquelch;
            app.radio.set_subaudio_scan_filter(syst, false);
        }
        KeyId::Menu if app.scanqt.phase == ScanQtPhase::Found => save(app, syst),
        KeyId::Exit => exit(app, syst),
        _ => {}
    }
}

// UI getters
pub(super) fn is_found(app: &App) -> bool {
    app.scanqt.phase == ScanQtPhase::Found
}

pub(super) fn is_listening(app: &App) -> bool {
    app.scanqt.phase == ScanQtPhase::Detecting
}

pub(super) fn tone(app: &App) -> Option<SubAudio> {
    app.scanqt.tone
}
