use super::chanmgr;
use super::contacts;
use super::fm;
use super::keyfn;
use super::launcher::{LauncherEntry, LAUNCHER_ITEMS};
use super::settings;
use super::settings_ops;
use super::{
    digit_value, App, ChVfoMode, DigitInput, Mode, BATTERY_CAL_REFERENCE_CV, CHANNEL_INPUT_DIGITS,
    DUAL_STANDBY_HOLD_TICKS, RTONE_HZ_DIV_10, VFO_INPUT_DIGITS, VOX_HOLD_AFTER_KEY_TICKS,
};
use super::{scan, scanqt, search, spectrum};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::device::radio::Modulation;
use cortex_m::peripheral::SYST;

// top-level dispatch
pub(super) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if app.key_lock
        && !matches!(ev.key, KeyId::Side1 | KeyId::Side2)
        && !matches!((ev.key, ev.kind), (KeyId::Asterisk, KeyEventKind::Long))
    {
        return;
    }

    app.reset_key_idle();
    app.vox_det_dly = VOX_HOLD_AFTER_KEY_TICKS;
    app.note_power_save_activity(syst);
    app.note_backlight_activity();

    if matches!(ev.key, KeyId::Side1 | KeyId::Side2 | KeyId::Band)
        && matches!(ev.kind, KeyEventKind::Press | KeyEventKind::Release)
        && keyfn::from_u8(short_function(app, ev.key)) == keyfn::KeyFunction::TxTone
    {
        dispatch_tx_tone(app, syst, ev);
        return;
    }

    // While transmitting, no other key does anything (the TX-tone key above
    // is the only exception).
    if app.transmitting && app.mode == Mode::Standby {
        return;
    }

    if matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Long) {
        app.radio.play_beep(syst);
    }

    match app.mode {
        Mode::Standby => dispatch_standby(app, syst, ev),
        Mode::AppMenu => dispatch_app_menu(app, syst, ev),
        Mode::Settings => dispatch_settings(app, syst, ev),
        Mode::ChanMgr => chanmgr::dispatch(app, ev),
        Mode::Contacts => contacts::dispatch(app, ev),
        Mode::Scan => scan::dispatch(app, syst, ev),
        Mode::Search => search::dispatch(app, syst, ev),
        Mode::ScanQt => scanqt::dispatch(app, syst, ev),
        Mode::Fm => fm::dispatch(app, syst, ev),
        Mode::Spectrum => spectrum::dispatch(app, syst, ev),
    }
}

fn dispatch_tx_tone(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match ev.kind {
        KeyEventKind::Press => {
            if !app.transmitting {
                app.set_ptt(syst, true);
                app.rtone_self_keyed = true;
            }
            if app.transmitting {
                let idx = app.settings.rtone.min(3) as usize;
                app.radio.rtone_on(syst, RTONE_HZ_DIV_10[idx]);
                app.rtone_sounding = true;
            }
        }
        KeyEventKind::Release => {
            if app.rtone_sounding && app.transmitting {
                app.radio.rtone_off(syst);
            }
            app.rtone_sounding = false;
            if app.rtone_self_keyed {
                app.rtone_self_keyed = false;
                app.set_ptt(syst, false);
            }
        }
        _ => {}
    }
}

fn dispatch_standby(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    app.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;

    if app.dtmf_dial.is_some() {
        dispatch_dtmf_dial(app, ev);
        return;
    }

    match ev.kind {
        KeyEventKind::Single => {
            if let Some(d) = digit_value(ev.key) {
                let max_len = match app.sides[app.master].vfo_chan {
                    ChVfoMode::Channel => CHANNEL_INPUT_DIGITS,
                    ChVfoMode::Vfo => VFO_INPUT_DIGITS,
                };
                app.input.push(d);
                if app.input.len >= max_len {
                    app.commit_input(syst);
                }
                return;
            }
            match ev.key {
                KeyId::Up => app.step(syst, true),
                KeyId::Down => app.step(syst, false),
                KeyId::Exit => app.input.clear(),
                KeyId::Vm => app.toggle_vfo_channel(syst),
                KeyId::Ab => app.switch_side(syst),
                // DTMF dialing doesn't make sense while keying CW (no mic
                // audio path at all in that mode); CWF still has one.
                KeyId::Asterisk if app.sides[app.master].cfg.modulation != Modulation::Cw => {
                    app.dtmf_dial = Some(DigitInput::new())
                }
                KeyId::Menu => {
                    if app.sides[app.master].vfo_chan == ChVfoMode::Channel && app.input.len > 0 {
                        app.commit_input(syst);
                    } else {
                        enter_app_menu(app);
                    }
                }
                KeyId::Side1 | KeyId::Side2 | KeyId::Band => {
                    let func = keyfn::from_u8(short_function(app, ev.key));
                    keyfn::invoke(app, syst, func);
                }
                _ => {}
            }
        }
        KeyEventKind::Repeat => match ev.key {
            KeyId::Up => app.step(syst, true),
            KeyId::Down => app.step(syst, false),
            _ => {}
        },
        KeyEventKind::Long => match ev.key {
            KeyId::Asterisk => app.key_lock = !app.key_lock,
            KeyId::Pound => scan::enter(app, syst),
            KeyId::Side1 | KeyId::Side2 | KeyId::Band => {
                let func = keyfn::from_u8(long_function(app, ev.key));
                keyfn::invoke(app, syst, func);
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_dtmf_dial(app: &mut App, ev: KeyEvent) {
    if ev.kind == KeyEventKind::Long && ev.key == KeyId::Exit {
        app.dtmf_dial = None;
        return;
    }
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(d) = digit_value(ev.key) {
        app.dtmf_dial.as_mut().unwrap().push(d);
        return;
    }
    let code = match ev.key {
        KeyId::Menu => Some(10),
        KeyId::Up => Some(11),
        KeyId::Down => Some(12),
        KeyId::Exit => Some(13),
        KeyId::Asterisk => Some(14),
        KeyId::Pound => Some(15),
        _ => None,
    };
    if let Some(code) = code {
        app.dtmf_dial.as_mut().unwrap().push(code);
    }
}

fn short_function(app: &App, key: KeyId) -> u8 {
    match key {
        KeyId::Side1 => app.settings.side1_short,
        KeyId::Side2 => app.settings.side2_short,
        KeyId::Band => app.settings.band_short,
        _ => 0,
    }
}

fn long_function(app: &App, key: KeyId) -> u8 {
    match key {
        KeyId::Side1 => app.settings.side1_long,
        KeyId::Side2 => app.settings.side2_long,
        KeyId::Band => app.settings.band_long,
        _ => 0,
    }
}

fn enter_app_menu(app: &mut App) {
    app.mode = Mode::AppMenu;
    app.input.clear();
}

fn dispatch_app_menu(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up | KeyId::Down => {
                let up = ev.key == KeyId::Up;
                let len = LAUNCHER_ITEMS.len();
                app.launcher_index = if up {
                    (app.launcher_index + len - 1) % len
                } else {
                    (app.launcher_index + 1) % len
                };
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                let entry = LAUNCHER_ITEMS[app.launcher_index];
                if entry.is_available() {
                    match entry {
                        LauncherEntry::Settings => settings_ops::enter(app),
                        LauncherEntry::ChannelMgr => chanmgr::enter(app),
                        LauncherEntry::Contacts => contacts::enter(app),
                        LauncherEntry::ScanQt => scanqt::enter(app, syst),
                        LauncherEntry::FmRadio => fm::enter(app, syst),
                        LauncherEntry::Search => search::enter(app, syst),
                        LauncherEntry::Spectrum => spectrum::enter(app, syst),
                    }
                    debug_assert!(app.mode == entry.target_mode());
                }
            }
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                app.mode = Mode::Standby;
                app.reset_key_idle();
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_settings(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    let item = settings::SETTINGS_ORDER[app.settings_ui.index];

    if app.settings_ui.editing && item == settings::SettingItem::Offse {
        dispatch_offset_input(app, syst, ev);
        return;
    }
    if app.settings_ui.editing && item == settings::SettingItem::BattCal {
        dispatch_battery_input(app, syst, ev);
        return;
    }

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up | KeyId::Down => {
                let up = ev.key == KeyId::Up;
                if !app.settings_ui.editing {
                    let len = settings::SETTINGS_ORDER.len();
                    app.settings_ui.index = if up {
                        (app.settings_ui.index + len - 1) % len
                    } else {
                        (app.settings_ui.index + 1) % len
                    };
                } else if item == settings::SettingItem::Info {
                    app.settings_ui.info_page ^= 1;
                } else if item != settings::SettingItem::Reset {
                    settings_ops::adjust(app, syst, item, up);
                }
            }
            KeyId::Digit0
                if ev.kind == KeyEventKind::Single
                    && app.settings_ui.editing
                    && item.is_scalar() =>
            {
                settings_ops::apply(app, syst, item, settings_ops::scalar_floor(item));
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                if !app.settings_ui.editing {
                    if item.is_placeholder() {
                        return;
                    }
                    if item == settings::SettingItem::Offse {
                        app.settings_ui.offset_input.clear();
                    } else if item == settings::SettingItem::BattCal {
                        app.settings_ui.battery_input.clear();
                    } else {
                        app.settings_ui.snapshot = settings_ops::current_value(app, item);
                    }
                    app.settings_ui.editing = true;
                } else if item == settings::SettingItem::Reset {
                    settings_ops::factory_reset(app);
                } else {
                    app.settings_ui.editing = false;
                    if item != settings::SettingItem::Info {
                        app.save_settings();
                    }
                }
            }
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                if app.settings_ui.editing {
                    if item != settings::SettingItem::Info && item != settings::SettingItem::Reset {
                        settings_ops::apply(app, syst, item, app.settings_ui.snapshot);
                    }
                    app.settings_ui.editing = false;
                } else {
                    settings_ops::exit(app);
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_offset_input(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(digit) = digit_value(ev.key) {
        app.settings_ui.offset_input.push(digit);
        if app.settings_ui.offset_input.is_full() {
            commit_offset_input(app, syst);
        }
        return;
    }
    match ev.key {
        KeyId::Menu => commit_offset_input(app, syst),
        KeyId::Exit if app.settings_ui.offset_input.is_empty() => {
            app.settings_ui.editing = false;
        }
        KeyId::Exit => app.settings_ui.offset_input.backspace(),
        _ => {}
    }
}

fn commit_offset_input(app: &mut App, syst: &mut SYST) {
    if !app.settings_ui.offset_input.is_empty() {
        let hz = app.settings_ui.offset_input.value() * 100;
        settings_ops::apply(app, syst, settings::SettingItem::Offse, hz as i32);
        app.save_settings();
    }
    app.settings_ui.offset_input.clear();
    app.settings_ui.editing = false;
}

fn dispatch_battery_input(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(digit) = digit_value(ev.key) {
        app.settings_ui.battery_input.push(digit);
        if app.settings_ui.battery_input.is_full() {
            commit_battery_input(app, syst);
        }
        return;
    }
    match ev.key {
        KeyId::Menu => commit_battery_input(app, syst),
        KeyId::Exit if app.settings_ui.battery_input.is_empty() => {
            app.settings_ui.editing = false;
        }
        KeyId::Exit => app.settings_ui.battery_input.backspace(),
        _ => {}
    }
}

/// Solves the calibration equation for the raw ADC value
fn commit_battery_input(app: &mut App, syst: &mut SYST) {
    let entered_cv = app.settings_ui.battery_input.value();
    let raw12 = app.battery_raw12 as u32;
    if let Some(new_cal) = (raw12 * BATTERY_CAL_REFERENCE_CV as u32).checked_div(entered_cv) {
        settings_ops::apply(
            app,
            syst,
            settings::SettingItem::BattCal,
            new_cal.clamp(1, u16::MAX as u32) as i32,
        );
        app.save_settings();
    }
    app.settings_ui.battery_input.clear();
    app.settings_ui.editing = false;
}
