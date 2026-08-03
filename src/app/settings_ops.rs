use super::keyfn;
use super::name_edit::write_name_plain;
use super::settings::SettingItem;
use super::{
    channel_display_mode_from_u8, clamp_step, power_from_raw, power_to_raw, subaudio_from_index,
    subaudio_index, wrap_step, App, ChVfoMode, Mode, FIRMWARE_VERSION, RTONE_HZ_DIV_10,
    STEP_LIST_DECI_HZ, SUBAUDIO_MAX_INDEX,
};
use crate::device::radio::{BandLock, Modulation, Power, RogerTone, SubAudio};
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
        SettingItem::Roge => app.settings.roger_tone as i32,
        SettingItem::Wn => !side.cfg.wide_band as i32,
        SettingItem::TxPr => power_to_raw(side.cfg.power) as i32,
        SettingItem::RxCts => subaudio_index(side.cfg.subaudio_rx),
        SettingItem::TxCts => subaudio_index(side.cfg.subaudio_tx),
        SettingItem::Scrm => app.settings.scramble_level as i32,
        SettingItem::Sftd => side.shift_dir_offset().0 as i32,
        SettingItem::Offse => side.shift_dir_offset().1 as i32,
        SettingItem::AutoLk => app.settings.key_auto_lock as i32,
        SettingItem::Vox => app.settings.vox_switch as i32,
        SettingItem::VoxLv => app.settings.vox_level as i32,
        SettingItem::VoxDly => app.settings.vox_delay as i32,
        SettingItem::Rtone => app.settings.rtone as i32,
        SettingItem::Tail => app.settings.tail_elimination as i32,
        SettingItem::Rptrl => app.settings.rptrl as i32,
        SettingItem::Contrast => app.settings.contrast as i32,
        SettingItem::ScanMd => app.settings.scan_mode as i32,
        SettingItem::Rit => app.settings.rit_offset as i32,
        SettingItem::Save => app.settings.save_level as i32,
        SettingItem::Abr => app.settings.backlight_time as i32,
        SettingItem::ChDisp => app.settings.channel_display_mode as i32,
        SettingItem::AniTx => app.settings.ani_tx as i32,
        SettingItem::AniCall => side.ani_target.map_or(-1, |t| t as i32),
        SettingItem::Side1Short => app.settings.side1_short as i32,
        SettingItem::Side1Long => app.settings.side1_long as i32,
        SettingItem::Side2Short => app.settings.side2_short as i32,
        SettingItem::Side2Long => app.settings.side2_long as i32,
        SettingItem::BandShort => app.settings.band_short as i32,
        SettingItem::BandLong => app.settings.band_long as i32,
        SettingItem::BkIn => app.settings.bk_in as i32,
        SettingItem::BootMode => app.settings.boot_display_mode as i32,
        SettingItem::BootSnd => app.settings.boot_sound_enabled as i32,
        SettingItem::BattCal => app.settings.battery_cal_raw as i32,
        SettingItem::FLock => app.settings.band_lock as i32,
        SettingItem::AprsFreq => app.settings.aprs_freq_hz as i32,
        SettingItem::AprsLat => app.settings.aprs_lat,
        SettingItem::AprsLon => app.settings.aprs_lon,
        SettingItem::AprsPath => app.settings.aprs_path_idx as i32,
        SettingItem::AprsDevInfo => app.settings.aprs_dev_info as i32,
        SettingItem::AprsBatVolt => app.settings.aprs_bat_volt as i32,
        SettingItem::AprsSsid => app.settings.aprs_ssid as i32,
        SettingItem::AprsSymbol => app.settings.aprs_symbol_idx as i32,
        SettingItem::AprsPower => app.settings.aprs_power as i32,
        SettingItem::Info => app.settings_ui.info_page as i32,
        _ => 0,
    }
}

pub(super) fn adjust(app: &mut App, syst: &mut SYST, item: SettingItem, up: bool) {
    let cur = current_value(app, item);

    let new_val = match item {
        SettingItem::Sql => clamp_step(cur, up, 0, 9),
        SettingItem::Step => clamp_step(cur, up, 0, STEP_LIST_DECI_HZ.len() as i32 - 1),
        SettingItem::Tot => clamp_step(cur, up, 0, 12),
        SettingItem::Sftd => clamp_step(cur, up, 0, 2),
        SettingItem::Tdr
        | SettingItem::BusyLock
        | SettingItem::TxForbid
        | SettingItem::Beep
        | SettingItem::Wn
        | SettingItem::Tail
        | SettingItem::AniTx
        | SettingItem::AprsDevInfo
        | SettingItem::AprsBatVolt => 1 - cur,
        // Locked to OFF while BootMode is None (0): boot sound has no
        // effect there and the item is meant to read as disabled, so
        // Up/Down are a no-op instead of toggling a value nobody can hear.
        SettingItem::BootSnd => {
            if app.settings.boot_display_mode == 0 {
                cur
            } else {
                1 - cur
            }
        }
        SettingItem::VoxDly => clamp_step(cur, up, 0, 15),
        SettingItem::Rptrl => clamp_step(cur, up, 0, 10),
        SettingItem::BootMode => clamp_step(cur, up, 0, 3),
        SettingItem::AniCall => clamp_step(
            cur,
            up,
            -1,
            crate::flash_map::addr::CONTACT_COUNT as i32 - 1,
        ),
        SettingItem::TxPr | SettingItem::Roge => clamp_step(cur, up, 0, 2),
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
        SettingItem::ChDisp => clamp_step(cur, up, 0, 2),
        SettingItem::Side1Short
        | SettingItem::Side1Long
        | SettingItem::Side2Short
        | SettingItem::Side2Long
        | SettingItem::BandShort
        | SettingItem::BandLong => clamp_step(cur, up, 0, 11),
        SettingItem::AprsPath => clamp_step(cur, up, 0, 3),
        SettingItem::AprsSsid => clamp_step(cur, up, 0, 15),
        SettingItem::AprsSymbol => clamp_step(cur, up, 0, 10),
        SettingItem::AprsPower => clamp_step(cur, up, 0, 2),
        SettingItem::FLock => clamp_step(cur, up, 0, 9),
        // Meaningless (and locked to OFF) while the master side isn't in CW
        SettingItem::BkIn => {
            if matches!(
                app.sides[app.master].cfg.modulation,
                Modulation::Cw | Modulation::Cwf
            ) {
                clamp_step(cur, up, 0, 2)
            } else {
                cur
            }
        }
        _ => cur,
    };
    apply(app, syst, item, new_val);
}

pub(super) fn scalar_floor(item: SettingItem) -> i32 {
    match item {
        SettingItem::AniCall | SettingItem::RxCts | SettingItem::TxCts => -1,
        SettingItem::VoxLv => 1,
        _ => 0,
    }
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
            app.set_dual_standby(syst, v != 0);
        }
        SettingItem::BusyLock => app.settings.busy_lock = v != 0,
        SettingItem::TxForbid => app.settings.tx_forbid = v != 0,
        SettingItem::Beep => {
            app.settings.beeps_switch = v != 0;
            app.radio.set_beeps_enabled(v != 0);
        }
        SettingItem::Roge => {
            app.settings.roger_tone = v as u8;
            app.radio.set_roger_tone(RogerTone::from_u8(v as u8));
        }
        SettingItem::Wn => {
            app.sides[app.master].cfg.wide_band = v == 0;
            app.commit_side_change(syst);
        }
        SettingItem::TxPr => {
            app.sides[app.master].cfg.power = power_from_raw(v as u8);
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
            let s = &mut app.sides[app.master];
            match s.vfo_chan {
                ChVfoMode::Vfo => s.freq_dir = v as u8,
                ChVfoMode::Channel => {
                    let (_, offset) = s.shift_dir_offset();
                    s.tx_freq_hz = match v {
                        1 => s.rx_freq_hz.saturating_add(offset),
                        2 => s.rx_freq_hz.saturating_sub(offset),
                        _ => s.rx_freq_hz,
                    };
                }
            }
            app.commit_side_change(syst);
        }
        SettingItem::Offse => {
            let s = &mut app.sides[app.master];
            match s.vfo_chan {
                ChVfoMode::Vfo => s.offset_hz = v as u32,
                ChVfoMode::Channel => {
                    let (dir, _) = s.shift_dir_offset();
                    s.tx_freq_hz = match dir {
                        1 => s.rx_freq_hz.saturating_add(v as u32),
                        2 => s.rx_freq_hz.saturating_sub(v as u32),
                        _ => s.rx_freq_hz,
                    };
                }
            }
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
        SettingItem::VoxDly => app.settings.vox_delay = v as u8,
        SettingItem::Rtone => app.settings.rtone = v as u8,
        SettingItem::Tail => {
            app.settings.tail_elimination = v != 0;
            app.radio.set_tail_elimination(v != 0);
        }
        SettingItem::Rptrl => {
            app.settings.rptrl = v as u8;
            app.radio.set_rptrl(v as u8);
        }
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
        SettingItem::ChDisp => {
            app.settings.channel_display_mode = v as u8;
            app.set_channel_display_mode(channel_display_mode_from_u8(v as u8));
        }
        SettingItem::AniTx => app.settings.ani_tx = v != 0,
        SettingItem::AniCall => {
            app.sides[app.master].ani_target = (v >= 0).then_some(v as u8);
            app.commit_side_change(syst);
        }
        SettingItem::Side1Short => app.settings.side1_short = v as u8,
        SettingItem::Side1Long => app.settings.side1_long = v as u8,
        SettingItem::Side2Short => app.settings.side2_short = v as u8,
        SettingItem::Side2Long => app.settings.side2_long = v as u8,
        SettingItem::BandShort => app.settings.band_short = v as u8,
        SettingItem::BandLong => app.settings.band_long = v as u8,
        SettingItem::BkIn => app.settings.bk_in = v as u8,
        SettingItem::BootMode => {
            app.settings.boot_display_mode = v as u8;
            if v == 0 {
                app.settings.boot_sound_enabled = false;
            }
        }
        SettingItem::BootSnd => app.settings.boot_sound_enabled = v != 0,
        SettingItem::BattCal => app.settings.battery_cal_raw = v as u16,
        SettingItem::FLock => {
            app.settings.band_lock = v as u8;
            app.radio
                .set_tx_allowed(BandLock::from_u8(v as u8).tx_ranges());
        }
        SettingItem::Rit => {
            app.settings.rit_offset = v as i8;
            app.radio.set_rit_offset(v * RIT_STEP_HZ);
            // Only actually affects a tuned frequency while listening in
            // USB, but re-tuning now (rather than waiting for the next
            // unrelated RX restart) makes it feel live while dialing it in.
            app.commit_side_change(syst);
        }
        SettingItem::AprsFreq => {
            app.settings.aprs_freq_hz = (v as u32).clamp(100_000_000, 999_999_999);
        }
        SettingItem::AprsLat => app.settings.aprs_lat = v,
        SettingItem::AprsLon => app.settings.aprs_lon = v,
        SettingItem::AprsPath => app.settings.aprs_path_idx = v as u8,
        SettingItem::AprsDevInfo => app.settings.aprs_dev_info = v != 0,
        SettingItem::AprsBatVolt => app.settings.aprs_bat_volt = v != 0,
        SettingItem::AprsSsid => app.settings.aprs_ssid = v as u8,
        SettingItem::AprsSymbol => app.settings.aprs_symbol_idx = v as u8,
        SettingItem::AprsPower => app.settings.aprs_power = v as u8,
        _ => {}
    }
}

// UI getters
pub fn value_text_for(app: &App, index: usize, item: SettingItem, w: &mut dyn Write) {
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
            if app.settings_ui.is_editing(index) {
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
        | SettingItem::Tail
        | SettingItem::BootSnd => {
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
        SettingItem::BootMode => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    1 => "VOLT",
                    2 => "MSG",
                    3 => "LOGO",
                    _ => "NONE",
                }
            );
        }
        SettingItem::BattCal => {
            if app.settings_ui.is_editing(index) {
                app.settings_ui.battery_input.write_display(1, w);
            } else {
                let cv = app.battery_voltage_cv();
                let _ = write!(w, "{}.{:02}V", cv / 100, cv % 100);
            }
        }
        SettingItem::VoxDly => {
            let deci_s = 5 + current_value(app, item);
            let _ = write!(w, "{}.{}S", deci_s / 10, deci_s % 10);
        }
        SettingItem::Rptrl => {
            let v = current_value(app, item);
            let _ = if v == 0 {
                write!(w, "OFF")
            } else {
                write!(w, "{}MS", v * 100)
            };
        }
        SettingItem::Roge => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    1 => "ROGER",
                    2 => "MDC1200",
                    _ => "OFF",
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
                match power_from_raw(current_value(app, item) as u8) {
                    Power::High => "HIGH",
                    Power::Low => "LOW",
                    Power::Mid => "MID",
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
            if app.settings_ui.is_editing(index) {
                app.settings_ui.offset_input.write_display(3, w);
            } else {
                let hz = current_value(app, item) as u32;
                let _ = write!(w, "{}.{:03}k", hz / 1000, hz % 1000);
            }
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
        SettingItem::ChDisp => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    1 => "NAME",
                    2 => "NAME+F",
                    _ => "FREQ",
                }
            );
        }
        SettingItem::Side1Short
        | SettingItem::Side1Long
        | SettingItem::Side2Short
        | SettingItem::Side2Long
        | SettingItem::BandShort
        | SettingItem::BandLong => {
            let idx = current_value(app, item).clamp(0, 11) as u8;
            let _ = write!(w, "{}", keyfn::from_u8(idx).label());
        }
        SettingItem::FLock => {
            let idx = current_value(app, item).clamp(0, 9) as u8;
            let _ = write!(w, "{}", BandLock::from_u8(idx).label());
        }
        SettingItem::BkIn => {
            let _ = write!(
                w,
                "{}",
                match current_value(app, item) {
                    1 => "SEMI",
                    2 => "FULL",
                    _ => "OFF",
                }
            );
        }
        SettingItem::AprsCall => {
            if app.settings_ui.is_editing(index) {
                write_name_plain(&app.settings_ui.aprs_call_edit.buf, w);
            } else {
                let call = &app.settings.aprs_callsign;
                let end = call.iter().position(|&b| b == 0).unwrap_or(call.len());
                for &b in &call[..end] {
                    let _ = w.write_char(if b == 0 || b == 0xFF { ' ' } else { b as char });
                }
            }
        }
        SettingItem::AprsFreq => {
            if app.settings_ui.is_editing(index) {
                app.settings_ui.aprs_freq_input.write_display(3, w);
            } else {
                let hz = app.settings.aprs_freq_hz;
                let mhz = hz / 1_000_000;
                let khz = (hz % 1_000_000) / 1000;
                let _ = write!(w, "{}.{:03}", mhz, khz);
            }
        }
        SettingItem::AprsLat | SettingItem::AprsLon => {
            let v = current_value(app, item);
            if v == crate::flash_map::APRS_COORD_NOT_SET {
                let _ = write!(w, "----");
            } else if app.settings_ui.is_editing(index) {
                match item {
                    SettingItem::AprsLat => {
                        app.settings_ui.aprs_lat_input.write_display(2, w);
                        let _ = write!(
                            w,
                            "{}",
                            if app.settings_ui.aprs_lat_neg { "S" } else { "N" }
                        );
                    }
                    _ /* AprsLon */ => {
                        app.settings_ui.aprs_lon_input.write_display(3, w);
                        let _ = write!(
                            w,
                            "{}",
                            if app.settings_ui.aprs_lon_neg { "W" } else { "E" }
                        );
                    }
                }
            } else {
                let neg = v < 0;
                let av = if neg { -v } else { v };
                let deg = av / 100_000;
                let frac = av % 100_000;
                let min = frac * 60 / 100_000;
                let min_frac = (frac * 60 % 100_000) / 1000;
                let _ = write!(
                    w,
                    "{}{}*{:02}.{:02}'",
                    if neg { "-" } else { "" },
                    deg,
                    min,
                    min_frac,
                );
            }
        }
        SettingItem::AprsPath => {
            let idx = current_value(app, item) as usize;
            let s = match idx {
                1 => "WIDE2-2",
                2 => "ARISS",
                3 => "DIRECT",
                _ => "W1-1,W2-1",
            };
            let _ = write!(w, "{}", s);
        }
        SettingItem::AprsDevInfo | SettingItem::AprsBatVolt => {
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
        SettingItem::AprsDevName => {
            if app.settings_ui.is_editing(index) {
                write_name_plain(&app.settings_ui.aprs_dev_name_edit.buf, w);
            } else {
                let name = &app.settings.aprs_dev_name;
                let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
                for &b in &name[..end] {
                    let _ = w.write_char(if b == 0 || b == 0xFF { ' ' } else { b as char });
                }
            }
        }
        SettingItem::AprsComment => {
            if app.settings_ui.is_editing(index) {
                write_name_plain(&app.settings_ui.aprs_comment_edit.buf, w);
            } else {
                let comment = &app.settings.aprs_custom_comment;
                let end = comment
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(comment.len());
                for &b in &comment[..end] {
                    let _ = w.write_char(if b == 0 || b == 0xFF { ' ' } else { b as char });
                }
            }
        }
        SettingItem::AprsSsid => {
            let _ = write!(w, "-{}", current_value(app, item));
        }
        SettingItem::AprsSymbol => {
            let idx = current_value(app, item) as usize;
            let label = super::aprs_ops::symbol_preset(idx as u8).2;
            let _ = write!(w, "{}", label);
        }
        SettingItem::AprsPower => {
            let label = match current_value(app, item) {
                1 => "MID",
                2 => "HIGH",
                _ => "LOW",
            };
            let _ = write!(w, "{}", label);
        }
        _ => {
            let _ = write!(w, "{}", current_value(app, item));
        }
    }
}
