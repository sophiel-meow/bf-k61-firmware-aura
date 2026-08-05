use super::{digit_value, App, ChVfoMode, Mode};
use crate::device::fm_radio::{FREQ_HI_KHZ, FREQ_LO_KHZ};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::flash_map;
use cortex_m::peripheral::SYST;

const FREQ_LO_DECI_MHZ: u16 = (FREQ_LO_KHZ / 100) as u16;
const FREQ_HI_DECI_MHZ: u16 = (FREQ_HI_KHZ / 100) as u16;
const FM_DEFAULT_DECI_MHZ: u16 = 1000; // 100.0MHz

const FREQ_INPUT_DIGITS: usize = 4; // deci-MHz, e.g. "1005" -> 100.5MHz
const CHANNEL_INPUT_DIGITS: usize = 2; // 01..=30

/// ~3s at the ~10ms/tick `poll_fm` is called on, in case the chip never
/// reports seek-complete.
const SEEK_TIMEOUT_TICKS: u16 = 300;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FmPhase {
    Tuning,
    Seeking { timeout: u16 },
    SavePicker { selected: u8 },
}

pub(super) struct FmState {
    mode: ChVfoMode,
    deci_mhz: u16,
    channel_index: u8,
    phase: FmPhase,
    rssi: u8,
}

impl FmState {
    pub(super) const fn new() -> Self {
        FmState {
            mode: ChVfoMode::Vfo,
            deci_mhz: FM_DEFAULT_DECI_MHZ,
            channel_index: 0,
            phase: FmPhase::Tuning,
            rssi: 0,
        }
    }
}

fn find_next_channel(
    channels: &[u16; flash_map::FM_CHANNEL_COUNT],
    from: u8,
    forward: bool,
) -> Option<u8> {
    let len = flash_map::FM_CHANNEL_COUNT as u8;
    let mut i = from;
    for _ in 0..len {
        i = if forward {
            if i + 1 >= len {
                0
            } else {
                i + 1
            }
        } else if i == 0 {
            len - 1
        } else {
            i - 1
        };
        if channels[i as usize] != flash_map::FM_CHANNEL_EMPTY {
            return Some(i);
        }
    }
    None
}

/// The slot whose stored frequency matches `deci_mhz`, or else the first
/// populated slot — used when switching from VFO into Channel mode.
fn find_matching_or_first(
    channels: &[u16; flash_map::FM_CHANNEL_COUNT],
    deci_mhz: u16,
) -> Option<u8> {
    channels
        .iter()
        .position(|&f| f == deci_mhz)
        .or_else(|| channels.iter().position(|&f| f != flash_map::FM_CHANNEL_EMPTY))
        .map(|i| i as u8)
}

fn tune(app: &mut App, syst: &mut SYST, deci_mhz: u16) {
    app.fm.deci_mhz = deci_mhz;
    app.fm_radio.tune_khz(syst, deci_mhz as u32 * 100);
}

pub(super) fn enter(app: &mut App, syst: &mut SYST) {
    app.fm = FmState::new();
    app.mode = Mode::Fm;
    app.input.clear();
    app.radio.park_for_fm(syst);
    let deci_mhz = app.fm.deci_mhz;
    tune(app, syst, deci_mhz);
    app.radio.set_speaker(true);
}

fn exit(app: &mut App, syst: &mut SYST) {
    app.fm_radio.power_off(syst);
    app.mode = Mode::Standby;
    app.reset_key_idle();
    app.sync_watching_to_master(syst);
}

pub(super) fn poll(app: &mut App, syst: &mut SYST) {
    if app.mode != Mode::Fm {
        return;
    }
    let (complete, seek_failed, _is_station, rssi) = app.fm_radio.status(syst);
    app.fm.rssi = rssi;

    let timeout = match app.fm.phase {
        FmPhase::Seeking { timeout } => timeout,
        _ => return,
    };
    if !complete && !seek_failed && timeout > 0 {
        if let FmPhase::Seeking { timeout } = &mut app.fm.phase {
            *timeout -= 1;
        }
        return;
    }
    if !seek_failed {
        let khz = app.fm_radio.tuned_frequency_khz(syst);
        app.fm.deci_mhz = ((khz / 100) as u16).clamp(FREQ_LO_DECI_MHZ, FREQ_HI_DECI_MHZ);
    }
    app.fm.phase = FmPhase::Tuning;
    app.radio.set_speaker(true);
}

fn step(app: &mut App, syst: &mut SYST, up: bool) {
    match app.fm.mode {
        ChVfoMode::Vfo => {
            let next = if up {
                if app.fm.deci_mhz >= FREQ_HI_DECI_MHZ {
                    FREQ_LO_DECI_MHZ
                } else {
                    app.fm.deci_mhz + 1
                }
            } else if app.fm.deci_mhz <= FREQ_LO_DECI_MHZ {
                FREQ_HI_DECI_MHZ
            } else {
                app.fm.deci_mhz - 1
            };
            tune(app, syst, next);
        }
        ChVfoMode::Channel => {
            if let Some(idx) = find_next_channel(&app.fm_channels, app.fm.channel_index, up) {
                app.fm.channel_index = idx;
                let freq = app.fm_channels[idx as usize];
                tune(app, syst, freq);
            }
        }
    }
}

fn start_seek(app: &mut App, syst: &mut SYST, up: bool) {
    app.radio.set_speaker(false);
    app.fm_radio.seek(syst, up);
    app.fm.phase = FmPhase::Seeking {
        timeout: SEEK_TIMEOUT_TICKS,
    };
}

fn toggle_mode(app: &mut App, syst: &mut SYST) {
    match app.fm.mode {
        ChVfoMode::Vfo => match find_matching_or_first(&app.fm_channels, app.fm.deci_mhz) {
            Some(idx) => {
                app.fm.mode = ChVfoMode::Channel;
                app.fm.channel_index = idx;
                let freq = app.fm_channels[idx as usize];
                tune(app, syst, freq);
            }
            None => app.radio.play_beep(syst),
        },
        ChVfoMode::Channel => {
            app.fm.mode = ChVfoMode::Vfo;
        }
    }
    app.input.clear();
}

fn handle_digit(app: &mut App, syst: &mut SYST, d: u8) {
    let max_len = if app.fm.mode == ChVfoMode::Vfo {
        FREQ_INPUT_DIGITS
    } else {
        CHANNEL_INPUT_DIGITS
    };
    if app.input.len >= max_len {
        app.input.clear();
    }
    app.input.push(d);
    if app.input.len == max_len {
        commit_input(app, syst);
    }
}

fn commit_input(app: &mut App, syst: &mut SYST) {
    let value = app.input.entered_value();
    app.input.clear();
    match app.fm.mode {
        ChVfoMode::Vfo => {
            let deci_mhz = value as u16;
            if (FREQ_LO_DECI_MHZ..=FREQ_HI_DECI_MHZ).contains(&deci_mhz) {
                tune(app, syst, deci_mhz);
            } else {
                app.radio.play_beep(syst);
            }
        }
        ChVfoMode::Channel => {
            let slot = value as usize;
            if (1..=flash_map::FM_CHANNEL_COUNT).contains(&slot)
                && app.fm_channels[slot - 1] != flash_map::FM_CHANNEL_EMPTY
            {
                app.fm.channel_index = (slot - 1) as u8;
                let freq = app.fm_channels[slot - 1];
                tune(app, syst, freq);
            } else {
                app.radio.play_beep(syst);
            }
        }
    }
}

fn open_save_picker(app: &mut App) {
    let selected = match app.fm.mode {
        ChVfoMode::Channel => app.fm.channel_index,
        ChVfoMode::Vfo => app
            .fm_channels
            .iter()
            .position(|&f| f == app.fm.deci_mhz)
            .map(|i| i as u8)
            .unwrap_or(0),
    };
    app.fm.phase = FmPhase::SavePicker { selected };
}

fn dispatch_save_picker(app: &mut App, ev: KeyEvent) {
    let selected = match app.fm.phase {
        FmPhase::SavePicker { selected } => selected,
        _ => return,
    };
    let len = flash_map::FM_CHANNEL_COUNT as u8;
    match (ev.kind, ev.key) {
        (KeyEventKind::Single, KeyId::Up) | (KeyEventKind::Repeat, KeyId::Up) => {
            let next = if selected == 0 { len - 1 } else { selected - 1 };
            app.fm.phase = FmPhase::SavePicker { selected: next };
        }
        (KeyEventKind::Single, KeyId::Down) | (KeyEventKind::Repeat, KeyId::Down) => {
            let next = if selected + 1 >= len { 0 } else { selected + 1 };
            app.fm.phase = FmPhase::SavePicker { selected: next };
        }
        (KeyEventKind::Single, KeyId::Menu) => {
            app.fm_channels[selected as usize] = app.fm.deci_mhz;
            let channels = app.fm_channels;
            app.storage.save_fm_channels(&channels);
            app.fm.phase = FmPhase::Tuning;
        }
        (KeyEventKind::Single, KeyId::Exit) | (KeyEventKind::Long, KeyId::Exit) => {
            app.fm.phase = FmPhase::Tuning;
        }
        _ => {}
    }
}

pub(super) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if matches!(app.fm.phase, FmPhase::SavePicker { .. }) {
        dispatch_save_picker(app, ev);
    } else if matches!(
        (ev.kind, ev.key),
        (KeyEventKind::Single, KeyId::Exit) | (KeyEventKind::Long, KeyId::Exit)
    ) {
        if app.input.len > 0 {
            app.input.clear();
        } else {
            exit(app, syst);
        }
    } else if matches!(app.fm.phase, FmPhase::Tuning) {
        if let Some(d) = digit_value(ev.key).filter(|_| ev.kind == KeyEventKind::Single) {
            handle_digit(app, syst, d);
        } else {
            match (ev.kind, ev.key) {
                (KeyEventKind::Single, KeyId::Vm) => toggle_mode(app, syst),
                (KeyEventKind::Single, KeyId::Up) | (KeyEventKind::Repeat, KeyId::Up) => {
                    step(app, syst, true)
                }
                (KeyEventKind::Single, KeyId::Down) | (KeyEventKind::Repeat, KeyId::Down) => {
                    step(app, syst, false)
                }
                (KeyEventKind::Long, KeyId::Up) => start_seek(app, syst, true),
                (KeyEventKind::Long, KeyId::Down) => start_seek(app, syst, false),
                (KeyEventKind::Single, KeyId::Menu) => open_save_picker(app),
                _ => {}
            }
        }
    }
    // else: mid-seek, only EXIT (handled above) does anything until it settles.

    // The shared key dispatcher plays a UI beep for every press/long-press
    // *before* calling us, and `Radio::play_beep` ends by restoring the
    // speaker-amp pin to `audio_open` — the two-way radio's own RX-squelch
    // state, which is always false while FM has it parked. That silently
    // re-mutes FM audio after literally every keypress (stepping, digit
    // entry, ...). Reassert it here whenever we're genuinely tuned in
    // (not mid-seek, where `start_seek` deliberately muted it).
    if app.mode == Mode::Fm && matches!(app.fm.phase, FmPhase::Tuning) {
        app.radio.set_speaker(true);
    }
}

// UI getters
pub(super) fn deci_mhz(app: &App) -> u16 {
    app.fm.deci_mhz
}

pub(super) fn is_channel_mode(app: &App) -> bool {
    app.fm.mode == ChVfoMode::Channel
}

pub(super) fn channel_index(app: &App) -> u8 {
    app.fm.channel_index
}

pub(super) fn is_seeking(app: &App) -> bool {
    matches!(app.fm.phase, FmPhase::Seeking { .. })
}

pub(super) fn rssi(app: &App) -> u8 {
    app.fm.rssi
}

pub(super) fn save_picker_selected(app: &App) -> Option<u8> {
    match app.fm.phase {
        FmPhase::SavePicker { selected } => Some(selected),
        _ => None,
    }
}

pub(super) fn channel_freq_at(app: &App, index: usize) -> Option<u16> {
    let f = app.fm_channels[index];
    if f == flash_map::FM_CHANNEL_EMPTY {
        None
    } else {
        Some(f)
    }
}

pub(super) fn input_len(app: &App) -> usize {
    app.input.len
}

pub(super) fn input_digit(app: &App, idx: usize) -> u8 {
    app.input.digits[idx]
}
