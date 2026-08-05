use cortex_m::peripheral::SYST;

use crate::device::radio::{Modulation, Power};

use super::convert::cw_safe_subaudio;
use super::side;
use super::{App, ChVfoMode, MAX_CHANNEL_NUM, NO_CHANNELS_HOLD_TICKS};

pub(super) fn push_watching_config(app: &mut App) {
    let s = &app.sides[app.watching];
    app.radio.set_frequency(s.cfg.freq_hz);
    app.radio.set_tx_frequency(s.cfg.tx_freq_hz);
    app.radio.set_power(s.cfg.power);
    let (subaudio_tx, subaudio_rx) = cw_safe_subaudio(&s.cfg);
    app.radio.set_subaudio_tx(subaudio_tx);
    app.radio.set_subaudio_rx(subaudio_rx);
    app.radio.set_modulation(s.cfg.modulation);
}

pub(super) fn apply_watching_to_radio(app: &mut App, syst: &mut SYST) {
    push_watching_config(app);
    if !app.transmitting {
        app.radio.enter_rx(syst);
    }
}

pub(super) fn sync_watching_to_master(app: &mut App, syst: &mut SYST) {
    app.watching = app.master;
    apply_watching_to_radio(app, syst);
}

pub(super) fn step(app: &mut App, syst: &mut SYST, up: bool) {
    let s = &mut app.sides[app.master];
    match s.vfo_chan {
        ChVfoMode::Vfo => {
            let step_hz = s.step_deci_hz() * 10;
            s.rx_freq_hz = if up {
                s.rx_freq_hz.saturating_add(step_hz)
            } else {
                s.rx_freq_hz.saturating_sub(step_hz)
            };
            s.refresh_cfg_freqs();
            app.save_vfo();
        }
        ChVfoMode::Channel => {
            let current = s.channel_num;
            if let Some(next) = find_programmed_channel(app, current, up) {
                load_channel_num(app, next);
            }
        }
    }
    sync_watching_to_master(app, syst);
}

pub(super) fn find_programmed_channel(app: &mut App, from: u16, up: bool) -> Option<u16> {
    let mut num = from;
    for _ in 0..=MAX_CHANNEL_NUM {
        num = if up {
            if num >= MAX_CHANNEL_NUM {
                0
            } else {
                num + 1
            }
        } else if num == 0 {
            MAX_CHANNEL_NUM
        } else {
            num - 1
        };
        if !app.storage.is_channel_empty(num) {
            return Some(num);
        }
    }
    None
}

pub(super) fn commit_input(app: &mut App, syst: &mut SYST) {
    let value = app.input.entered_value();
    match app.sides[app.master].vfo_chan {
        ChVfoMode::Channel => {
            if value <= MAX_CHANNEL_NUM as u32 && !app.storage.is_channel_empty(value as u16) {
                load_channel_num(app, value as u16);
                sync_watching_to_master(app, syst);
            }
        }
        ChVfoMode::Vfo => {
            let s = &mut app.sides[app.master];
            s.rx_freq_hz = value * 1000;
            s.refresh_cfg_freqs();
            app.save_vfo();
            sync_watching_to_master(app, syst);
        }
    }
    app.input.clear();
}

pub(super) fn toggle_vfo_channel(app: &mut App, syst: &mut SYST) {
    let s = &mut app.sides[app.master];
    match s.vfo_chan {
        ChVfoMode::Vfo => {
            s.vfo_backup = side::VfoBackup {
                rx_freq_hz: s.rx_freq_hz,
                freq_dir: s.freq_dir,
                offset_hz: s.offset_hz,
                wide_band: s.cfg.wide_band,
                power: s.cfg.power,
                subaudio_tx: s.cfg.subaudio_tx,
                subaudio_rx: s.cfg.subaudio_rx,
                ani_target: s.ani_target,
            };
            let num = s.channel_num;
            let num = if app.storage.is_channel_empty(num) {
                find_programmed_channel(app, num, true)
            } else {
                Some(num)
            };
            if let Some(num) = num {
                load_channel_num(app, num);
            } else {
                app.no_channels_ticks = NO_CHANNELS_HOLD_TICKS;
            }
        }
        ChVfoMode::Channel => {
            let backup = s.vfo_backup;
            s.vfo_chan = ChVfoMode::Vfo;
            s.name = [0; 12];
            s.rx_freq_hz = backup.rx_freq_hz;
            s.freq_dir = backup.freq_dir;
            s.offset_hz = backup.offset_hz;
            s.cfg.wide_band = backup.wide_band;
            s.cfg.power = backup.power;
            s.cfg.subaudio_tx = backup.subaudio_tx;
            s.cfg.subaudio_rx = backup.subaudio_rx;
            s.ani_target = backup.ani_target;
            s.refresh_cfg_freqs();
        }
    }

    app.input.clear();
    app.ani_target_override = None;
    sync_watching_to_master(app, syst);
}

pub(super) fn switch_side(app: &mut App, syst: &mut SYST) {
    app.master = 1 - app.master;
    app.input.clear();
    app.ani_target_override = None;
    sync_watching_to_master(app, syst);
}

pub(super) fn toggle_reverse(app: &mut App, syst: &mut SYST) {
    let s = &mut app.sides[app.master];
    s.reversed = !s.reversed;
    s.refresh_cfg_freqs();
    sync_watching_to_master(app, syst);
}

pub(super) fn toggle_power(app: &mut App, syst: &mut SYST) {
    let s = &mut app.sides[app.master];
    s.cfg.power = match s.cfg.power {
        Power::High => Power::Low,
        Power::Low => Power::Mid,
        Power::Mid => Power::High,
    };
    sync_watching_to_master(app, syst);
}

pub(super) fn toggle_wide_narrow(app: &mut App, syst: &mut SYST) {
    let s = &mut app.sides[app.master];
    s.cfg.wide_band = !s.cfg.wide_band;
    commit_side_change(app, syst);
}

pub(super) fn toggle_modulation(app: &mut App, syst: &mut SYST) {
    if app.cw_tx_active {
        app.radio.enter_rx(syst);
        app.cw_tx_active = false;
    }
    app.cw_hang_ticks = 0;
    app.cw_key_down = false;
    let s = &mut app.sides[app.master];
    s.cfg.modulation = match s.cfg.modulation {
        Modulation::Fm => Modulation::Am,
        Modulation::Am => Modulation::Usb,
        Modulation::Usb => Modulation::Cw,
        Modulation::Cw => Modulation::Cwf,
        Modulation::Cwf => Modulation::Fm,
    };
    sync_watching_to_master(app, syst);
}

pub(super) fn commit_side_change(app: &mut App, syst: &mut SYST) {
    app.sides[app.master].refresh_cfg_freqs();
    if matches!(app.sides[app.master].vfo_chan, ChVfoMode::Vfo) {
        app.save_vfo();
    }
    sync_watching_to_master(app, syst);
}

pub(super) fn load_channel_num(app: &mut App, num: u16) {
    let ch = app.storage.read_channel(num);
    app.sides[app.master].load_channel(num, &ch);
    app.ani_target_override = None;
}

pub(super) fn refresh_channel_display(app: &mut App, num: u16) {
    for i in 0..app.sides.len() {
        let showing =
            app.sides[i].vfo_chan == ChVfoMode::Channel && app.sides[i].channel_num == num;
        if !showing {
            continue;
        }
        if app.storage.is_channel_empty(num) {
            if let Some(next) = find_programmed_channel(app, num, true) {
                let ch = app.storage.read_channel(next);
                app.sides[i].load_channel(next, &ch);
            }
        } else {
            let ch = app.storage.read_channel(num);
            app.sides[i].load_channel(num, &ch);
        }
    }
}
