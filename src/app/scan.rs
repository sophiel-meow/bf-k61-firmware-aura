use super::{App, ChVfoMode, Mode, MAX_CHANNEL_NUM};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::device::radio::FreqRanges;
use cortex_m::peripheral::SYST;

const SCAN_LOST_TIME_TICKS: u16 = 100; // 5.0s: (50 @ 100ms)
const SCAN_FIND_TIME_TICKS: u16 = 20; // 1.0s: (10 @ 100ms)
const SCAN_STEP_DWELL_TICKS: u16 = 4; // 200ms
const SCAN_START_DWELL_TICKS: u16 = 8; // 400ms
const SQUELCH_DEBOUNCE_TICKS: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanPhase {
    Scanning,
    WaitTime,
    WaitLost,
    WaitRecall,
    Stop,
}

pub(super) struct ScanState {
    phase: ScanPhase,
    up: bool,
    dwell_ticks: u16,
    sq_debounce: u8,
}

impl ScanState {
    pub(super) const fn new() -> Self {
        ScanState {
            phase: ScanPhase::Scanning,
            up: true,
            dwell_ticks: 0,
            sq_debounce: 0,
        }
    }
}

fn find_next_scan_channel(app: &mut App, from: u16, forward: bool) -> Option<u16> {
    let mut n = from;
    for _ in 0..MAX_CHANNEL_NUM {
        n = if forward {
            if n >= MAX_CHANNEL_NUM {
                1
            } else {
                n + 1
            }
        } else if n <= 1 {
            MAX_CHANNEL_NUM
        } else {
            n - 1
        };
        if app.storage.read_channel(n).scan_add() {
            return Some(n);
        }
    }
    None
}

fn wrap_freq(freq_hz: u32, step_hz: u32, up: bool, lo: u32, hi: u32) -> u32 {
    if up {
        let next = freq_hz.saturating_add(step_hz);
        if next > hi {
            lo
        } else {
            next
        }
    } else if freq_hz < step_hz || freq_hz - step_hz < lo {
        hi
    } else {
        freq_hz - step_hz
    }
}

pub(super) fn enter(app: &mut App, syst: &mut SYST) {
    let master = &app.sides[app.master];
    if master.vfo_chan == ChVfoMode::Channel
        && find_next_scan_channel(app, master.channel_num, true).is_none()
    {
        // TODO: play error beep tone
        app.radio.play_beep(syst);
        return;
    }

    app.scan = ScanState {
        phase: ScanPhase::Scanning,
        up: true,
        dwell_ticks: SCAN_START_DWELL_TICKS,
        sq_debounce: 0,
    };
    app.mode = Mode::Scan;
    app.input.clear();
    app.sync_watching_to_master(syst);
}

fn hop(app: &mut App, syst: &mut SYST) {
    let up = app.scan.up;
    match app.sides[app.master].vfo_chan {
        ChVfoMode::Channel => {
            let from = app.sides[app.master].channel_num;
            if let Some(next) = find_next_scan_channel(app, from, up) {
                app.load_channel_num(next);
            }
        }
        ChVfoMode::Vfo => {
            let (lo, hi) = FreqRanges::hardware().bounds();
            let s = &mut app.sides[app.master];
            let step_hz = s.step_deci_hz() * 10;
            s.rx_freq_hz = wrap_freq(s.rx_freq_hz, step_hz, up, lo, hi);
            s.refresh_cfg_freqs();
        }
    }
    app.scan.phase = ScanPhase::Scanning;
    app.scan.dwell_ticks = SCAN_STEP_DWELL_TICKS;
    app.scan.sq_debounce = 0;
    app.apply_watching_to_radio(syst);
}

fn exit(app: &mut App, syst: &mut SYST) {
    app.mode = Mode::Standby;
    app.reset_key_idle();
    app.sync_watching_to_master(syst);
}

pub(super) fn poll(app: &mut App, syst: &mut SYST) {
    if app.mode != Mode::Scan {
        return;
    }
    let squelch_open = app.radio.rssi_open();

    match app.scan.phase {
        ScanPhase::Scanning => {
            if app.scan.dwell_ticks > 0 {
                app.scan.dwell_ticks -= 1;
                return;
            }
            if squelch_open {
                if app.scan.sq_debounce < SQUELCH_DEBOUNCE_TICKS {
                    app.scan.sq_debounce += 1;
                    return;
                }
                match app.settings.scan_mode {
                    0 => {
                        app.scan.phase = ScanPhase::WaitTime;
                        app.scan.dwell_ticks = SCAN_LOST_TIME_TICKS;
                    }
                    2 => {
                        app.scan.phase = ScanPhase::Stop;
                        app.scan.dwell_ticks = SCAN_FIND_TIME_TICKS;
                    }
                    _ => {
                        app.scan.phase = ScanPhase::WaitLost;
                        app.scan.dwell_ticks = SCAN_LOST_TIME_TICKS;
                    }
                }
                return;
            }
            hop(app, syst);
        }
        // Fixed hold, then move on regardless of whether the carrier is
        // still present ("Time" mode).
        ScanPhase::WaitTime => {
            if app.scan.dwell_ticks > 0 {
                app.scan.dwell_ticks -= 1;
                return;
            }
            hop(app, syst);
        }
        // Pause indefinitely while carrier is present; once it drops, wait
        // out a grace period before resuming ("Carrier" mode).
        ScanPhase::WaitLost => {
            if squelch_open {
                app.scan.dwell_ticks = SCAN_LOST_TIME_TICKS;
            } else if app.scan.dwell_ticks > 0 {
                app.scan.dwell_ticks -= 1;
                if app.scan.dwell_ticks == 0 {
                    app.scan.phase = ScanPhase::WaitRecall;
                }
            }
        }
        ScanPhase::WaitRecall => hop(app, syst),
        // Found a signal in "Search" mode: hold briefly, then stop
        // scanning entirely.
        ScanPhase::Stop => {
            if app.scan.dwell_ticks > 0 {
                app.scan.dwell_ticks -= 1;
                return;
            }
            exit(app, syst);
        }
    }
}

pub(super) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match (ev.kind, ev.key) {
        (KeyEventKind::Single, KeyId::Up) => {
            app.scan.up = true;
            hop(app, syst);
        }
        (KeyEventKind::Single, KeyId::Down) => {
            app.scan.up = false;
            hop(app, syst);
        }
        (KeyEventKind::Long, KeyId::Pound)
        | (KeyEventKind::Single, KeyId::Exit)
        | (KeyEventKind::Long, KeyId::Exit) => exit(app, syst),
        _ => {}
    }
}

pub(super) fn direction_up(app: &App) -> bool {
    app.scan.up
}
