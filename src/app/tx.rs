use cortex_m::peripheral::SYST;

use crate::device::radio::Power;

use super::{
    App, Mode, CW_FULL_HANG_TICKS, CW_SEMI_HANG_TICKS, CW_SIDETONE_HZ_DIV_10,
    DUAL_STANDBY_HOLD_TICKS, TICKS_PER_SECOND, VOX_HOLD_AFTER_PTT_TICKS,
};

pub(super) fn set_ani_target_override(app: &mut App, id: [u8; 3]) {
    app.ani_target_override = Some(id);
}

fn resolve_ani_target(app: &mut App) -> Option<[u8; 3]> {
    if let Some(id) = app.ani_target_override {
        return Some(id);
    }
    let idx = app.sides[app.master].ani_target?;
    let contact = app.storage.read_contact(idx);
    (!contact.is_empty()).then(|| contact.id())
}

fn reload_pa_calibration(app: &mut App, tx_freq_hz: u32, power: Power) {
    app.radio
        .apply_pa_calibration(&mut app.storage, tx_freq_hz, power);
}

pub(super) fn set_ptt(app: &mut App, syst: &mut SYST, pressed: bool) {
    app.note_power_save_activity(syst);
    app.note_backlight_activity();
    if matches!(
        app.sides[app.master].cfg.modulation,
        crate::device::radio::Modulation::Cw | crate::device::radio::Modulation::Cwf
    ) {
        set_cw_key(app, syst, pressed);
        return;
    }
    if pressed && !app.transmitting {
        if app.mode != Mode::Standby {
            return;
        }
        if app.settings.tx_forbid {
            return;
        }
        if app.settings.busy_lock && app.radio.rssi_open() {
            return;
        }

        app.watching = app.master;
        let s = &app.sides[app.master];
        let tx_freq_hz = s.cfg.tx_freq_hz;
        let power = s.cfg.power;
        app.radio.set_frequency(s.cfg.freq_hz);
        app.radio.set_tx_frequency(tx_freq_hz);
        app.radio.set_power(power);
        app.radio.set_subaudio_tx(s.cfg.subaudio_tx);
        app.radio.set_subaudio_rx(s.cfg.subaudio_rx);
        app.radio.set_modulation(s.cfg.modulation);
        reload_pa_calibration(app, tx_freq_hz, power);
        if app.radio.enter_tx(syst) {
            app.transmitting = true;
            app.tot_ticks = 0;
            app.tx_prohibited = false;
            if let Some(dial) = app.dtmf_dial.take() {
                let digits = &dial.digits[..dial.len];
                if dial.len == 3 {
                    let target = [digits[0], digits[1], digits[2]];
                    app.ani_target_override = Some(target);
                    app.radio.send_ani(syst, target);
                } else if dial.len > 0 {
                    app.radio.send_dtmf_digits(syst, digits);
                }
            } else if app.settings.ani_tx {
                if let Some(target) = resolve_ani_target(app) {
                    app.radio.send_ani(syst, target);
                }
            }
        } else {
            app.tx_prohibited = true;
        }
    } else if pressed {
        app.vox_active = false;
        app.vox_work_dly = 0;
    } else {
        // PTT released.
        app.tx_prohibited = false;
        if app.transmitting {
            app.transmitting = false;
            if app.rtone_sounding {
                app.rtone_sounding = false;
            }
            app.radio.end_tx(syst);
            app.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
            app.vox_det_dly = VOX_HOLD_AFTER_PTT_TICKS;
            app.vox_work_dly = 0;
            app.vox_active = false;
        }
    }
}

fn set_cw_key(app: &mut App, syst: &mut SYST, down: bool) {
    use crate::device::radio::Modulation;

    if app.mode != Mode::Standby {
        return;
    }
    let is_cw = app.sides[app.master].cfg.modulation == Modulation::Cw;
    if down {
        app.cw_key_down = true;
        app.cw_hang_ticks = 0;
        if !app.cw_tx_active {
            let want_tx = app.settings.bk_in != 0
                && !app.settings.tx_forbid
                && !(app.settings.busy_lock && app.radio.rssi_open());
            if want_tx {
                app.watching = app.master;
                let s = &app.sides[app.master];
                let tx_freq_hz = s.cfg.tx_freq_hz;
                let power = s.cfg.power;
                app.radio.set_frequency(s.cfg.freq_hz);
                app.radio.set_tx_frequency(tx_freq_hz);
                app.radio.set_power(power);
                reload_pa_calibration(app, tx_freq_hz, power);
                app.cw_tx_active = app.radio.enter_tx_keyed(syst);
                if app.cw_tx_active {
                    app.tot_ticks = 0;
                }
            }
        }
        if app.cw_tx_active {
            app.radio.keyed_tone_on(syst, CW_SIDETONE_HZ_DIV_10);
            if is_cw {
                app.radio.cw_carrier_on(syst);
            }
        } else {
            app.radio.sidetone_on(syst, CW_SIDETONE_HZ_DIV_10);
        }
    } else {
        app.cw_key_down = false;
        if app.cw_tx_active {
            if is_cw {
                app.radio.cw_carrier_off(syst);
                app.radio.cw_key_off(syst);
            } else {
                app.radio.keyed_tone_off(syst);
            }
            app.cw_hang_ticks = match app.settings.bk_in {
                2 => CW_FULL_HANG_TICKS,
                1 => CW_SEMI_HANG_TICKS,
                _ => 0,
            };
        } else {
            app.radio.sidetone_off(syst);
        }
    }
}

/// Raw (undebounced) key-edge poll for CW.
pub(super) fn poll_cw_key(app: &mut App, syst: &mut SYST) {
    let down = app.radio.ptt_asserted();
    if down != app.cw_key_down {
        set_cw_key(app, syst, down);
    }
}

/// BK-IN=Semi's post-release hang timer.
pub(super) fn poll_cw_hang(app: &mut App, syst: &mut SYST) {
    if app.cw_hang_ticks == 0 {
        return;
    }
    app.cw_hang_ticks -= 1;
    if app.cw_hang_ticks == 0 && !app.cw_key_down {
        app.radio.enter_rx(syst);
        app.cw_tx_active = false;
    }
}

pub(super) fn poll_tot(app: &mut App, syst: &mut SYST) {
    if !app.is_transmitting() || app.settings.tot_level == 0 {
        return;
    }
    app.tot_ticks += 1;
    let limit = app.settings.tot_seconds() * TICKS_PER_SECOND;
    if app.tot_ticks >= limit {
        if app.cw_tx_active {
            app.radio.enter_rx(syst);
            app.cw_tx_active = false;
            app.cw_hang_ticks = 0;
        } else {
            set_ptt(app, syst, false);
        }
    }
}
