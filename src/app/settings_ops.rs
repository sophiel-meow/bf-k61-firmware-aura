use super::settings::SettingItem;
use super::{
    clamp_step, subaudio_from_index, subaudio_index, wrap_step, App, ChVfoMode, Mode,
    FIRMWARE_VERSION, RTONE_HZ_DIV_10, STEP_LIST_DECI_HZ, SUBAUDIO_MAX_INDEX,
};
use crate::device::radio::{Power, SubAudio};
use crate::flash_map;
use core::fmt::Write;
use cortex_m::peripheral::{SCB, SYST};

const RIT_STEP_HZ: i32 = 10;

pub(super) fn enter(app: &mut App) {
    app.mode = Mode::Settings;
    app.settings_ui.editing = false;
    app.input.clear();
}

pub(super) fn exit(app: &mut App) {
    app.save_settings();
    app.save_vfo();
    app.mode = Mode::Standby;
    app.reset_key_idle();
}

pub(super) fn factory_reset(app: &mut App) -> ! {
    app.storage.factory_reset();
    SCB::sys_reset();
}

pub(super) fn current_value(app: &App, item: SettingItem) -> i32 {
    let side = &app.sides[app.master];
    match item {
        SettingItem::Sql => app.settings.sql_level as i32,
        SettingItem::Step => side.freq_step as i32,
        SettingItem::Tot => app.settings.tot_level as i32,
        SettingItem::Tdr => app.settings.dual_standby as i32,
        SettingItem::BusyLock => app.settings.busy_lock as i32,
        SettingItem::TxForbid => app.settings.tx_forbid as i32,
        SettingItem::Beep => app.settings.beeps_switch as i32,
        SettingItem::Roge => app.settings.roger_beep as i32,
        SettingItem::Wn => !side.cfg.wide_band as i32,
        SettingItem::TxPr => matches!(side.cfg.power, Power::Low) as i32,
        SettingItem::RxCts => subaudio_index(side.cfg.subaudio_rx),
        SettingItem::TxCts => subaudio_index(side.cfg.subaudio_tx),
        SettingItem::Scrm => app.settings.scramble_level as i32,
        SettingItem::Sftd => side.freq_dir as i32,
        SettingItem::Offse => side.offset_hz as i32,
        SettingItem::AutoLk => app.settings.key_auto_lock as i32,
        SettingItem::Vox => app.settings.vox_switch as i32,
        SettingItem::VoxLv => app.settings.vox_level as i32,
        SettingItem::Rtone => app.settings.rtone as i32,
        SettingItem::Contrast => app.settings.contrast as i32,
        SettingItem::ScanMd => app.settings.scan_mode as i32,
        SettingItem::Rit => app.settings.rit_offset as i32,
        SettingItem::Save => app.settings.save_level as i32,
        SettingItem::Abr => app.settings.backlight_time as i32,
        SettingItem::Info => app.settings_ui.info_page as i32,
        _ => 0,
    }
}

pub(super) fn adjust(app: &mut App, syst: &mut SYST, item: SettingItem, up: bool) {
    let cur = current_value(app, item);
    let side_step_hz = app.sides[app.master].step_deci_hz() * 10;

    let new_val = match item {
        SettingItem::Sql => clamp_step(cur, up, 0, 9),
        SettingItem::Step => clamp_step(cur, up, 0, STEP_LIST_DECI_HZ.len() as i32 - 1),
        SettingItem::Tot => clamp_step(cur, up, 0, 12),
        SettingItem::Sftd => clamp_step(cur, up, 0, 2),
        SettingItem::Tdr
        | SettingItem::BusyLock
        | SettingItem::TxForbid
        | SettingItem::Beep
        | SettingItem::Roge
        | SettingItem::Wn
        | SettingItem::TxPr => 1 - cur,
        SettingItem::RxCts | SettingItem::TxCts => wrap_step(cur, up, -1, SUBAUDIO_MAX_INDEX),
        SettingItem::Scrm => clamp_step(cur, up, 0, 3),
        SettingItem::AutoLk => clamp_step(cur, up, 0, 3),
        SettingItem::Vox => 1 - cur,
        SettingItem::VoxLv => clamp_step(cur, up, 1, 9),
        SettingItem::Rtone => clamp_step(cur, up, 0, 3),
        SettingItem::Contrast => clamp_step(cur, up, 0, 4),
        SettingItem::ScanMd => clamp_step(cur, up, 0, 2),
        SettingItem::Rit => clamp_step(cur, up, -127, 127),
        SettingItem::Save => clamp_step(cur, up, 0, 4),
        SettingItem::Abr => clamp_step(cur, up, 0, 4),
        SettingItem::Offse => {
            if up {
                cur.saturating_add(side_step_hz as i32)
            } else {
                (cur - side_step_hz as i32).max(0)
            }
        }
        _ => cur,
    };
    apply(app, syst, item, new_val);
}

pub(super) fn apply(app: &mut App, syst: &mut SYST, item: SettingItem, v: i32) {
    match item {
        SettingItem::Sql => {
            app.settings.sql_level = v as u8;
            app.radio.set_sql_level(syst, v as u8);
        }
        SettingItem::Step => app.sides[app.master].freq_step = v as u8,
        SettingItem::Tot => app.settings.tot_level = v as u8,
        SettingItem::Tdr => {
            app.settings.dual_standby = v != 0;
            app.set_dual_standby(v != 0);
        }
        SettingItem::BusyLock => app.settings.busy_lock = v != 0,
        SettingItem::TxForbid => app.settings.tx_forbid = v != 0,
        SettingItem::Beep => {
            app.settings.beeps_switch = v != 0;
            app.radio.set_beeps_enabled(v != 0);
        }
        SettingItem::Roge => {
            app.settings.roger_beep = v != 0;
            app.radio.set_roger_beep(v != 0);
        }
        SettingItem::Wn => {
            app.sides[app.master].cfg.wide_band = v == 0;
            app.commit_side_change(syst);
        }
        SettingItem::TxPr => {
            app.sides[app.master].cfg.power = if v != 0 { Power::Low } else { Power::High };
            app.commit_side_change(syst);
        }
        SettingItem::RxCts => {
            app.sides[app.master].cfg.subaudio_rx = subaudio_from_index(v);
            app.commit_side_change(syst);
        }
        SettingItem::TxCts => {
            app.sides[app.master].cfg.subaudio_tx = subaudio_from_index(v);
            app.commit_side_change(syst);
        }
        SettingItem::Scrm => {
            app.settings.scramble_level = v as u8;
            app.radio.set_scramble_level(syst, v as u8);
        }
        SettingItem::Sftd => {
            app.sides[app.master].freq_dir = v as u8;
            app.commit_side_change(syst);
        }
        SettingItem::Offse => {
            app.sides[app.master].offset_hz = v as u32;
            app.commit_side_change(syst);
        }
        SettingItem::AutoLk => {
            app.settings.key_auto_lock = v as u8;
            app.reset_key_idle();
        }
        SettingItem::Vox => {
            app.settings.vox_switch = v != 0;
            if v == 0 && app.vox_active {
                app.vox_active = false;
                app.vox_work_dly = 0;
                app.set_ptt(syst, false);
            }
        }
        SettingItem::VoxLv => app.settings.vox_level = v as u8,
        SettingItem::Rtone => app.settings.rtone = v as u8,
        SettingItem::Contrast => app.settings.contrast = v as u8,
        SettingItem::ScanMd => app.settings.scan_mode = v as u8,
        SettingItem::Save => {
            app.settings.save_level = v as u8;
            app.reset_power_save(syst);
        }
        SettingItem::Abr => {
            app.settings.backlight_time = v as u8;
            app.note_backlight_activity();
        }
        SettingItem::Rit => {
            app.settings.rit_offset = v as i8;
            app.radio.set_rit_offset(v * RIT_STEP_HZ);
            // Only actually affects a tuned frequency while listening in
            // USB, but re-tuning now (rather than waiting for the next
            // unrelated RX restart) makes it feel live while dialing it in.
            app.commit_side_change(syst);
        }
        _ => {}
    }
}

// UI getters
pub fn editing(app: &App) -> bool {
    app.settings_ui.editing
}

pub fn value_text_for(app: &App, item: SettingItem, w: &mut dyn Write) {
    if item.is_placeholder() {
        let _ = write!(w, "----");
        return;
    }
    match item {
        SettingItem::Info => {
            let _ = if app.settings_ui.info_page == 0 {
                write!(w, "Aura {}", FIRMWARE_VERSION)
            } else {
                write!(w, "UV-K6x")
            };
        }
        SettingItem::Reset => {
            if app.settings_ui.editing {
                let _ = write!(w, "Sure? MENU");
            }
        }
        SettingItem::Step => {
            let deci_hz = STEP_LIST_DECI_HZ[current_value(app, item) as usize];
            let _ = write!(w, "{}.{:02}k", deci_hz / 100, deci_hz % 100);
        }
        SettingItem::Tdr
        | SettingItem::BusyLock
        | SettingItem::TxForbid
        | SettingItem::Beep
        | SettingItem::Roge => {
            let _ = write!(
                w,
                "{}",
                if current_value(app, item) != 0 {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }
        SettingItem::AutoLk => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    1 => "5S",
                    2 => "10S",
                    3 => "15S",
                    _ => "OFF",
                }
            );
        }
        SettingItem::Vox => {
            let _ = write!(
                w,
                "{}",
                if current_value(app, item) != 0 {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }
        SettingItem::Rtone => {
            let idx = current_value(app, item).clamp(0, 3) as usize;
            let _ = write!(w, "{}Hz", RTONE_HZ_DIV_10[idx] as u32 * 10);
        }
        SettingItem::Wn => {
            let _ = write!(
                w,
                "{}",
                if current_value(app, item) != 0 {
                    "NARROW"
                } else {
                    "WIDE"
                }
            );
        }
        SettingItem::TxPr => {
            let _ = write!(
                w,
                "{}",
                if current_value(app, item) != 0 {
                    "LOW"
                } else {
                    "HIGH"
                }
            );
        }
        SettingItem::RxCts | SettingItem::TxCts => {
            let side = &app.sides[app.master];
            let sub = if item == SettingItem::RxCts {
                side.cfg.subaudio_rx
            } else {
                side.cfg.subaudio_tx
            };
            match sub {
                SubAudio::None => {
                    let _ = write!(w, "OFF");
                }
                SubAudio::Ctcss(hz) => {
                    let _ = write!(w, "{}.{}Hz", hz / 10, hz % 10);
                }
                SubAudio::Dcs { code, inverted } => {
                    let _ = write!(w, "D{:03o}{}", code, if inverted { "I" } else { "N" });
                }
            }
        }
        SettingItem::Scrm => {
            let v = current_value(app, item);
            if v == 0 {
                let _ = write!(w, "OFF");
            } else {
                let _ = write!(w, "{}", v);
            }
        }
        SettingItem::Offse => {
            let hz = current_value(app, item) as u32;
            let _ = write!(w, "{}.{:03}k", hz / 1000, hz % 1000);
        }
        SettingItem::ScanMd => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    0 => "TIME",
                    2 => "STOP",
                    _ => "CARR",
                }
            );
        }
        SettingItem::Rit => {
            let hz = current_value(app, item) * RIT_STEP_HZ;
            let _ = if hz == 0 {
                write!(w, "0Hz")
            } else {
                write!(w, "{:+}Hz", hz)
            };
        }
        SettingItem::Save => {
            let v = current_value(app, item);
            let _ = if v == 0 {
                write!(w, "OFF")
            } else {
                write!(w, "{}", v)
            };
        }
        SettingItem::Abr => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    1 => "5S",
                    2 => "10S",
                    3 => "15S",
                    4 => "20S",
                    _ => "OFF",
                }
            );
        }
        _ => {
            let _ = write!(w, "{}", current_value(app, item));
        }
    }
}
