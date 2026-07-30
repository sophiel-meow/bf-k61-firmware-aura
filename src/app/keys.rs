use super::launcher::{LauncherEntry, LAUNCHER_ITEMS};
use super::settings;
use super::settings_ops;
use super::{scan, scanqt, search};
use super::{
    digit_value, App, ChVfoMode, Mode, CHANNEL_INPUT_DIGITS, DUAL_STANDBY_HOLD_TICKS,
    RTONE_HZ_DIV_10, VFO_INPUT_DIGITS, VOX_HOLD_AFTER_KEY_TICKS,
};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use core::fmt::Write;
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

    // TX + Standby: only Side2 access tone works.
    if app.transmitting && app.mode == Mode::Standby {
        dispatch_tx(app, syst, ev);
        return;
    }

    if matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Long) {
        app.radio.play_beep(syst);
    }

    match app.mode {
        Mode::Standby => dispatch_standby(app, syst, ev),
        Mode::AppMenu => dispatch_app_menu(app, syst, ev),
        Mode::Settings => dispatch_settings(app, syst, ev),
        Mode::Scan => scan::dispatch(app, syst, ev),
        Mode::Search => search::dispatch(app, syst, ev),
        Mode::ScanQt => scanqt::dispatch(app, syst, ev),
        _ => {}
    }
}

fn dispatch_tx(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    if ev.key != KeyId::Side2 {
        return;
    }
    match ev.kind {
        KeyEventKind::Press => {
            let idx = app.settings.rtone.min(3) as usize;
            app.radio.rtone_on(syst, RTONE_HZ_DIV_10[idx]);
            app.rtone_sounding = true;
        }
        KeyEventKind::Release => {
            if app.rtone_sounding {
                app.radio.rtone_off(syst);
                app.rtone_sounding = false;
            }
        }
        _ => {}
    }
}

fn dispatch_standby(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    app.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;

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
                KeyId::Asterisk => app.toggle_reverse(syst),
                KeyId::Band => app.toggle_modulation(syst),
                KeyId::Menu => enter_app_menu(app),
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
            KeyId::Digit8 => app.toggle_power(syst),
            KeyId::Digit9 => app.test_send_dtmf(syst),
            KeyId::Band => search::enter(app, syst),
            KeyId::Pound => scan::enter(app, syst),
            _ => {}
        },
        _ => {}
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
                        LauncherEntry::ScanQt => scanqt::enter(app, syst),
                        _ => app.mode = entry.target_mode(),
                    }
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

pub(super) fn launcher_value_text<W: Write>(app: &App, w: &mut W) {
    let entry = LAUNCHER_ITEMS[app.launcher_index];
    let _ = if entry.is_available() {
        write!(w, "{}", entry.label())
    } else {
        write!(w, "{} ----", entry.label())
    };
}

fn dispatch_settings(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    let item = settings::SETTINGS_ORDER[app.settings_ui.index];

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up | KeyId::Down => {
                let up = ev.key == KeyId::Up;
                if !app.settings_ui.editing {
                    let len = settings::SETTINGS_ORDER.len();
                    app.settings_ui.index = if up {
                        (app.settings_ui.index + 1) % len
                    } else {
                        (app.settings_ui.index + len - 1) % len
                    };
                } else if item == settings::SettingItem::Info {
                    app.settings_ui.info_page ^= 1;
                } else if item != settings::SettingItem::Reset {
                    settings_ops::adjust(app, syst, item, up);
                }
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                if !app.settings_ui.editing {
                    if item.is_placeholder() {
                        return;
                    }
                    app.settings_ui.snapshot = settings_ops::current_value(app, item);
                    app.settings_ui.editing = true;
                } else if item == settings::SettingItem::Reset {
                    settings_ops::factory_reset(app);
                } else {
                    app.settings_ui.editing = false;
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
