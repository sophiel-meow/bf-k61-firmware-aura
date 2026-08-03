use super::ax25::aprs_packet::AprsConfig;
use super::ax25::ax25::{self, Ax25Builder};
use super::App;
use crate::flash_map::APRS_COORD_NOT_SET;

pub const DIGIPATH_PRESETS: [&[([u8; 6], u8)]; 4] = [
    &[(*b"WIDE1 ", 1), (*b"WIDE2 ", 1)],
    &[(*b"WIDE2 ", 2)],
    &[(*b"ARISS ", 0)],
    &[],
];

pub fn digipath_preset(idx: u8) -> &'static [([u8; 6], u8)] {
    let i = (idx as usize).min(DIGIPATH_PRESETS.len() - 1);
    DIGIPATH_PRESETS[i]
}

pub const APRS_SYMBOLS: &[(u8, u8, &str)] = &[
    (b'/', b'[', "PERSON"),     // Person / Runner
    (b'/', b';', "HANDHELD"),   // Portable handheld radio
    (b'/', b'>', "CAR"),        // Car
    (b'/', b'k', "TRUCK"),      // Truck
    (b'/', b'b', "BICYCLE"),    // Bicycle
    (b'/', b'-', "HOUSE"),      // House / QTH
    (b'/', b'_', "WEATHER"),    // Weather station
    (b'/', b'#', "DIGIPEATER"), // Digipeater
    (b'/', b'&', "HF_GATEWAY"), // HF gateway
    (b'/', b'O', "BALLOON"),    // Balloon
    (b'/', b'\'', "AIRCRAFT"),  // Small aircraft
];

pub fn symbol_preset(idx: u8) -> (u8, u8, &'static str) {
    let i = (idx as usize).min(APRS_SYMBOLS.len() - 1);
    APRS_SYMBOLS[i]
}

const NRZI_BUF_SIZE: usize = 512;

pub(super) fn send_beacon(app: &mut App) {
    if app.is_transmitting() || app.mode() != super::Mode::Standby {
        return;
    }

    let s = &app.settings;

    if s.aprs_lat == APRS_COORD_NOT_SET || s.aprs_lon == APRS_COORD_NOT_SET {
        return;
    }
    if s.aprs_callsign.iter().all(|&b| b == 0 || b == 0xFF) {
        return;
    }

    let src_call = pad6(&s.aprs_callsign);
    let mut config = AprsConfig::new(src_call, s.aprs_ssid);
    config.lat = s.aprs_lat;
    config.lon = s.aprs_lon;

    let (sym_table, sym_code, _) = symbol_preset(s.aprs_symbol_idx);
    config.symbol_table = sym_table;
    config.symbol_code = sym_code;

    let path = digipath_preset(s.aprs_path_idx);
    config.digi_count = (path.len() as u8).min(4);
    for (i, &(call, ssid)) in path.iter().enumerate() {
        config.digi_path[i] = (call, ssid);
    }

    let mut comment_buf = [0u8; 44];
    let mut pos: usize = 0;

    if s.aprs_dev_info {
        let name = &s.aprs_dev_name;
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        if end > 0 {
            comment_buf[pos..pos + end].copy_from_slice(&name[..end]);
            pos += end;
            comment_buf[pos] = b' ';
            pos += 1;
        }
    }

    if s.aprs_bat_volt {
        let cv = app.battery_voltage_cv();
        let v_int = cv / 100;
        let v_frac = (cv % 100) / 10;
        if v_int >= 10 {
            comment_buf[pos] = b'0' + ((v_int / 10) % 10) as u8;
            pos += 1;
        }
        comment_buf[pos] = b'0' + (v_int % 10) as u8;
        pos += 1;
        comment_buf[pos] = b'.';
        pos += 1;
        comment_buf[pos] = b'0' + v_frac as u8;
        pos += 1;
        comment_buf[pos] = b'V';
        pos += 1;
        comment_buf[pos] = b' ';
        pos += 1;
    }

    let custom = &s.aprs_custom_comment;
    let c_end = custom.iter().position(|&b| b == 0).unwrap_or(custom.len());
    if c_end > 0 {
        comment_buf[pos..pos + c_end].copy_from_slice(&custom[..c_end]);
        pos += c_end;
    }

    if pos > 0 && comment_buf[pos - 1] == b' ' {
        pos -= 1;
    }

    config.comment = comment_buf;
    config.comment_len = pos as u8;

    let mut builder = Ax25Builder::new();
    config.build_position_report(&mut builder);

    let mut nrzi_buf = [0u8; NRZI_BUF_SIZE];
    let total_bits = ax25::nrzi_encode_frame(builder.as_bytes(), 3, &mut nrzi_buf);

    app.set_aprs_beacon_pending(nrzi_buf, total_bits);
}

fn pad6(raw: &[u8; 7]) -> [u8; 6] {
    let mut out = [b' '; 6];
    for (i, &b) in raw.iter().take(6).enumerate() {
        if b == 0 {
            break;
        }
        out[i] = b.to_ascii_uppercase();
    }
    out
}
