use super::tracking;
use super::{DetailField, SatellitePage, SatelliteUi};
use crate::app::digit_value;
use crate::app::settings::ctcss_index;
use crate::app::settings::CTCSS_TABLE;
use crate::app::{App, Mode};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::flash_map::{MAX_SATELLITES, SatRecord};
use cortex_m::peripheral::SYST;

pub(crate) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match app.mode {
        Mode::Satellite => dispatch_page(app, syst, ev),
        Mode::SatelliteTracking => dispatch_tracking(app, syst, ev),
        _ => {}
    }
}

fn dispatch_page(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match app.satellite.page {
        SatellitePage::List => dispatch_list(app, ev),
        SatellitePage::Detail => dispatch_detail(app, syst, ev),
        SatellitePage::TimeSet => dispatch_time_set(app, ev),
        SatellitePage::Tracking => {}
    }
}

fn dispatch_list(app: &mut App, ev: KeyEvent) {
    let ui = &mut app.satellite;
    let sats = &mut app.sats;

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up | KeyId::Down => {
                if ui.confirm_delete {
                    if ev.key == KeyId::Down {
                        // Delete confirmed
                        if let Some(idx) = ui.sat_index_for_list_pos(sats) {
                            if idx < MAX_SATELLITES && sats[idx].is_some() {
                                sats[idx] = None;
                                ui.confirm_delete = false;
                                if ui.list_index >= SatelliteUi::list_item_count(sats) {
                                    ui.list_index = 0;
                                }
                                app.save_satellites();
                                return;
                            }
                        }
                    }
                    ui.confirm_delete = false;
                    return;
                }

                if ev.key == KeyId::Up {
                    ui.list_up(sats);
                } else {
                    ui.list_down(sats);
                }
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                if ui.list_index == 0 {
                    ui.enter_time_set();
                } else if let Some(sat_idx) = ui.sat_index_for_list_pos(sats) {
                    ui.enter_detail(sat_idx, false);
                } else {
                    let empty = SatelliteUi::first_empty_index(sats);
                    if empty < MAX_SATELLITES {
                        sats[empty] = Some(SatRecord::BLANK);
                        ui.enter_detail(empty, true);
                    }
                }
            }
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                if ui.confirm_delete {
                    ui.confirm_delete = false;
                    return;
                }
                ui.page = SatellitePage::List;
                app.mode = Mode::Standby;
                app.reset_key_idle();
            }
            KeyId::Asterisk if ev.kind == KeyEventKind::Long => {
                if let Some(idx) = ui.sat_index_for_list_pos(sats) {
                    if idx < MAX_SATELLITES && sats[idx].is_some() {
                        ui.confirm_delete = true;
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_detail(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    let ui = &mut app.satellite;
    let sats = &mut app.sats;
    let sat = sats[ui.detail_index].get_or_insert(SatRecord::BLANK);
    let time_set = app.time_set;

    if ui.editing && ui.detail_field == DetailField::Delete {
        if ev.kind == KeyEventKind::Single {
            match ev.key {
                KeyId::Menu => {
                    sats[ui.detail_index] = None;
                    ui.page = SatellitePage::List;
                    ui.editing = false;
                    app.save_satellites();
                }
                KeyId::Exit => {
                    ui.editing = false;
                }
                _ => {}
            }
        }
        return;
    }

    if ui.editing {
        if dispatch_detail_edit(ui, sat, ev) {
            app.save_satellites();
        }
        return;
    }

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up => {
                ui.detail_field = ui.detail_field.prev(ui.is_new);
            }
            KeyId::Down => {
                ui.detail_field = ui.detail_field.next(ui.is_new);
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => match ui.detail_field {
                DetailField::StartTracking => {
                    if time_set && ui.pass_configured {
                        start_tracking(app, syst);
                    }
                }
                DetailField::Save => {
                    if ui.is_new {
                        ui.is_new = false;
                    }
                    ui.page = SatellitePage::List;
                    app.save_satellites();
                }
                DetailField::Delete => {
                    ui.editing = true;
                }
                DetailField::Name => {
                    ui.name_edit.start(sat.name);
                    ui.editing = true;
                }
                DetailField::RxFreq | DetailField::TxFreq => {
                    ui.freq_edit.clear();
                    ui.editing = true;
                }
                DetailField::RxTone => {
                    ui.tone_idx = tone_value_to_index(sat.rx_tone_hz);
                    ui.tone_snapshot = ui.tone_idx;
                    ui.editing = true;
                }
                DetailField::TxTone => {
                    ui.tone_idx = tone_value_to_index(sat.tx_tone_hz);
                    ui.tone_snapshot = ui.tone_idx;
                    ui.editing = true;
                }
                DetailField::Altitude => {
                    ui.alt_edit.clear();
                    ui.editing = true;
                }
                DetailField::MaxEl => {
                    ui.el_edit.clear();
                    ui.editing = true;
                }
                DetailField::Aos | DetailField::Los => {
                    ui.time_edit.clear();
                    ui.editing = true;
                }
            },
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                if ui.is_new {
                    sats[ui.detail_index] = None;
                }
                ui.page = SatellitePage::List;
                app.save_satellites();
            }
            _ => {}
        },
        _ => {}
    }
}

fn start_tracking(app: &mut App, syst: &mut SYST) {
    let ui = &mut app.satellite;
    if let Some((doppler_state, rx_freq_hz, _tx_freq_hz, _band)) =
        ui.start_tracking(&app.sats, app.wall_secs)
    {
        app.doppler = Some(doppler_state);
        app.mode = Mode::SatelliteTracking;
        ui.tracking_sql = 0;
        ui.tracking_monitor = false;
        ui.tracking_wide = true;
        ui.gain_menu = 0;
        app.radio.set_frequency(rx_freq_hz);
        app.radio.enter_rx(syst);
        app.radio.set_sql_level(syst, ui.tracking_sql);
        app.radio.set_monitor(false);
    }
}

fn dispatch_detail_edit(ui: &mut SatelliteUi, sat: &mut SatRecord, ev: KeyEvent) -> bool {
    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Exit => match ui.detail_field {
                DetailField::Name => {
                    if ui.name_edit.buf.iter().all(|&b| b == 0) {
                        ui.editing = false;
                    } else {
                        ui.name_edit.backspace();
                    }
                }
                DetailField::RxFreq | DetailField::TxFreq => {
                    if ui.freq_edit.is_empty() {
                        ui.editing = false;
                    } else {
                        ui.freq_edit.backspace();
                    }
                }
                DetailField::Aos | DetailField::Los => {
                    if ui.time_edit.is_empty() {
                        ui.editing = false;
                    } else {
                        ui.time_edit.backspace();
                    }
                }
                DetailField::Altitude => {
                    if ui.alt_edit.is_empty() {
                        ui.editing = false;
                    } else {
                        ui.alt_edit.backspace();
                    }
                }
                DetailField::MaxEl => {
                    if ui.el_edit.is_empty() {
                        ui.editing = false;
                    } else {
                        ui.el_edit.backspace();
                    }
                }
                DetailField::RxTone | DetailField::TxTone => {
                    ui.tone_idx = ui.tone_snapshot;
                    ui.editing = false;
                }
                _ => {
                    ui.editing = false;
                }
            },

            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                match ui.detail_field {
                    DetailField::Name => {
                        sat.name = ui.name_edit.buf;
                    }
                    DetailField::RxFreq | DetailField::TxFreq => {
                        if !ui.freq_edit.is_empty() {
                            commit_freq(ui, sat);
                        }
                        ui.freq_edit.clear();
                    }
                    DetailField::Aos | DetailField::Los => {
                        if !ui.time_edit.is_empty() {
                            commit_time_edit(ui);
                        }
                        ui.time_edit.clear();
                    }
                    DetailField::Altitude => {
                        if !ui.alt_edit.is_empty() {
                            commit_alt(ui, sat);
                        }
                        ui.alt_edit.clear();
                    }
                    DetailField::MaxEl => {
                        if !ui.el_edit.is_empty() {
                            commit_el(ui);
                        }
                        ui.el_edit.clear();
                    }
                    DetailField::RxTone | DetailField::TxTone => {
                        commit_tone(ui, sat);
                    }
                    _ => {}
                }
                ui.editing = false;
                return true;
            }

            KeyId::Up | KeyId::Down => match ui.detail_field {
                DetailField::RxTone | DetailField::TxTone => {
                    cycle_tone(ui, ev.key == KeyId::Up);
                }
                _ => {}
            },

            key_id => {
                if let Some(digit) = digit_value(key_id) {
                    match ui.detail_field {
                        DetailField::RxFreq | DetailField::TxFreq => {
                            ui.freq_edit.push(digit);
                            if ui.freq_edit.is_full() {
                                commit_freq(ui, sat);
                                ui.freq_edit.clear();
                                ui.editing = false;
                                return true;
                            }
                        }
                        DetailField::Aos | DetailField::Los => {
                            ui.time_edit.push(digit);
                            if ui.time_edit.is_full() {
                                commit_time_edit(ui);
                                ui.time_edit.clear();
                                ui.editing = false;
                                return true;
                            }
                        }
                        DetailField::Altitude => {
                            ui.alt_edit.push(digit);
                            if ui.alt_edit.is_full() {
                                commit_alt(ui, sat);
                                ui.alt_edit.clear();
                                ui.editing = false;
                                return true;
                            }
                        }
                        DetailField::MaxEl => {
                            ui.el_edit.push(digit);
                            if ui.el_edit.is_full() {
                                commit_el(ui);
                                ui.el_edit.clear();
                                ui.editing = false;
                                return true;
                            }
                        }
                        DetailField::Name => {
                            ui.name_edit.press_digit(digit);
                        }
                        _ => {}
                    }
                }
            }
        },
        _ => {}
    }
    false
}

fn dispatch_time_set(app: &mut App, ev: KeyEvent) {
    let ui = &mut app.satellite;

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                // Commit wall-clock time
                let h = ui.time_h.value() as u8;
                let m = ui.time_m.value() as u8;
                let s = ui.time_s.value() as u8;
                if h < 24 && m < 60 && s < 60 {
                    app.wall_secs = super::time::wall_secs_from_hms(h, m, s);
                    app.time_set = true;
                }
                ui.page = SatellitePage::List;
                app.mode = Mode::Satellite;
            }
            KeyId::Exit => {
                time_backspace(ui);
            }
            KeyId::Up => {
                ui.time_field = match ui.time_field {
                    0 => 2,
                    1 => 0,
                    _ => 1,
                };
            }
            KeyId::Down => {
                ui.time_field = (ui.time_field + 1) % 3;
            }
            key_id => {
                if let Some(digit) = digit_value(key_id) {
                    time_push_digit(ui, digit);
                }
            }
        },
        KeyEventKind::Long if ev.key == KeyId::Exit => {
            ui.page = SatellitePage::List;
            app.mode = Mode::Satellite;
        }
        _ => {}
    }
}

fn dispatch_tracking(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Exit => {
                if app.satellite.gain_menu > 0 {
                    app.satellite.gain_menu = 0;
                } else {
                    app.radio.end_tx(syst);
                    app.doppler = None;
                    app.satellite.page = SatellitePage::Detail;
                    app.mode = Mode::Satellite;
                    app.push_watching_config();
                    app.radio.enter_rx(syst);
                    app.radio.set_sql_level(syst, app.settings.sql_level);
                    app.radio.set_monitor(false);
                }
            }
            KeyId::Up => {
                if app.satellite.gain_menu > 0 {
                    tracking::tracking_adjust_gain(
                        &mut app.radio,
                        syst,
                        app.satellite.gain_menu,
                        true,
                    );
                    let (lnas, lna, pga, if_gain) =
                        tracking::tracking_read_gains(&mut app.radio, syst);
                    app.satellite.gain_lnas = lnas;
                    app.satellite.gain_lna = lna;
                    app.satellite.gain_pga = pga;
                    app.satellite.gain_if = if_gain;
                } else if app.satellite.tracking_monitor {
                    app.satellite.tracking_monitor = false;
                    app.satellite.tracking_sql = 0;
                    tracking::tracking_set_squelch(
                        &mut app.radio,
                        syst,
                        app.satellite.tracking_sql,
                        app.satellite.tracking_monitor,
                    );
                } else {
                    app.satellite.tracking_sql =
                        app.satellite.tracking_sql.saturating_add(1).min(9);
                    tracking::tracking_set_squelch(
                        &mut app.radio,
                        syst,
                        app.satellite.tracking_sql,
                        app.satellite.tracking_monitor,
                    );
                }
            }
            KeyId::Down => {
                if app.satellite.gain_menu > 0 {
                    tracking::tracking_adjust_gain(
                        &mut app.radio,
                        syst,
                        app.satellite.gain_menu,
                        false,
                    );
                    let (lnas, lna, pga, if_gain) =
                        tracking::tracking_read_gains(&mut app.radio, syst);
                    app.satellite.gain_lnas = lnas;
                    app.satellite.gain_lna = lna;
                    app.satellite.gain_pga = pga;
                    app.satellite.gain_if = if_gain;
                } else if app.satellite.tracking_monitor {
                    // OFF stays OFF
                } else if app.satellite.tracking_sql == 0 {
                    // 0 -> OFF (monitor)
                    app.satellite.tracking_monitor = true;
                    tracking::tracking_set_squelch(
                        &mut app.radio,
                        syst,
                        app.satellite.tracking_sql,
                        app.satellite.tracking_monitor,
                    );
                } else {
                    app.satellite.tracking_sql = app.satellite.tracking_sql.saturating_sub(1);
                    tracking::tracking_set_squelch(
                        &mut app.radio,
                        syst,
                        app.satellite.tracking_sql,
                        app.satellite.tracking_monitor,
                    );
                }
            }
            KeyId::Menu => {
                if app.satellite.gain_menu >= 4 {
                    app.satellite.gain_menu = 0;
                } else {
                    app.satellite.gain_menu += 1;
                }
            }
            KeyId::Band if ev.kind == KeyEventKind::Single => {
                app.satellite.tracking_wide = !app.satellite.tracking_wide;
            }
            _ => {}
        },
        _ => {}
    }
}

fn time_push_digit(ui: &mut SatelliteUi, digit: u8) {
    let (input, max_val, next_field) = match ui.time_field {
        0 => (&mut ui.time_h, 24u32, 1u8),
        1 => (&mut ui.time_m, 60u32, 2u8),
        _ => (&mut ui.time_s, 60u32, 0u8),
    };

    if input.is_full() {
        input.clear();
    }

    input.push(digit);

    if input.is_full() {
        if input.value() >= max_val {
            input.backspace();
        } else {
            ui.time_field = next_field;
        }
    }
}

fn time_backspace(ui: &mut SatelliteUi) {
    // Search from seconds → minutes → hours for the first non-empty field
    for (field_idx, empty) in [
        (2u8, ui.time_s.is_empty()),
        (1u8, ui.time_m.is_empty()),
        (0u8, ui.time_h.is_empty()),
    ] {
        if !empty {
            match field_idx {
                2 => ui.time_s.backspace(),
                1 => ui.time_m.backspace(),
                _ => ui.time_h.backspace(),
            }
            ui.time_field = field_idx;
            return;
        }
    }

    // All empty -> return to list
    ui.page = SatellitePage::List;
}

fn commit_freq(ui: &mut SatelliteUi, sat: &mut SatRecord) {
    let khz = ui.freq_edit.value();
    let hz = khz * 1000;
    match ui.detail_field {
        DetailField::RxFreq => {
            if hz >= 10_000_000 {
                sat.rx_freq_hz = hz;
            }
        }
        DetailField::TxFreq if hz == 0 || hz >= 10_000_000 => {
            sat.tx_freq_hz = hz;
        }
        _ => {}
    }
}

fn commit_time_edit(ui: &mut SatelliteUi) {
    let v = ui.time_edit.value();
    let h = ((v / 10000) % 100) as u8;
    let m = ((v / 100) % 100) as u8;
    let s = (v % 100) as u8;
    match ui.detail_field {
        DetailField::Aos => {
            ui.aos_h = h.min(23);
            ui.aos_m = m.min(59);
            ui.aos_s = s.min(59);
            ui.pass_configured = true;
        }
        DetailField::Los => {
            ui.los_h = h.min(23);
            ui.los_m = m.min(59);
            ui.los_s = s.min(59);
            ui.pass_configured = true;
        }
        _ => {}
    }
}

fn commit_alt(ui: &mut SatelliteUi, sat: &mut SatRecord) {
    let v: u32 = ui.alt_edit.digits[..ui.alt_edit.len]
        .iter()
        .fold(0u32, |acc, &d| acc * 10 + d as u32);
    sat.altitude_km = v.min(9999) as u16;
}

fn commit_el(ui: &mut SatelliteUi) {
    let v: u32 = ui.el_edit.digits[..ui.el_edit.len]
        .iter()
        .fold(0u32, |acc, &d| acc * 10 + d as u32);
    ui.max_el_deg = v.min(90) as u16;
}

fn commit_tone(ui: &mut SatelliteUi, sat: &mut SatRecord) {
    let tenth_hz = if ui.tone_idx < 0 {
        0
    } else {
        CTCSS_TABLE[ui.tone_idx as usize]
    };
    match ui.detail_field {
        DetailField::RxTone => sat.rx_tone_hz = tenth_hz,
        DetailField::TxTone => sat.tx_tone_hz = tenth_hz,
        _ => {}
    }
}

fn cycle_tone(ui: &mut SatelliteUi, up: bool) {
    let max = CTCSS_TABLE.len() as i16 - 1;
    ui.tone_idx = if up {
        if ui.tone_idx >= max {
            -1
        } else {
            ui.tone_idx + 1
        }
    } else {
        if ui.tone_idx <= -1 {
            max
        } else {
            ui.tone_idx - 1
        }
    };
}

fn tone_value_to_index(tenth_hz: u16) -> i16 {
    if tenth_hz == 0 {
        return -1;
    }
    match ctcss_index(Some(tenth_hz)) {
        Some(idx) => idx as i16,
        None => -1,
    }
}
