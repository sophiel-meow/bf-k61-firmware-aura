use crate::device::keypad::KeyId;
use crate::device::radio::{ChannelConfig, Modulation, Power, SubAudio};
use crate::flash_map;

use super::settings;

use super::ChannelDisplayMode;

pub(super) fn channel_display_mode_from_u8(v: u8) -> ChannelDisplayMode {
    match v {
        1 => ChannelDisplayMode::Name,
        2 => ChannelDisplayMode::NameFreq,
        _ => ChannelDisplayMode::Frequency,
    }
}

pub(super) fn channel_name_str(raw: &[u8; 12]) -> &str {
    let end = raw
        .iter()
        .position(|&b| b == 0x00 || b == 0xFF)
        .unwrap_or(raw.len());
    core::str::from_utf8(&raw[..end]).unwrap_or("")
}

pub(super) fn subaudio_from_code(code: u16) -> SubAudio {
    match flash_map::SubaudioCode::decode(code) {
        flash_map::SubaudioCode::None => SubAudio::None,
        flash_map::SubaudioCode::Ctcss(t) => SubAudio::Ctcss(t),
        flash_map::SubaudioCode::DcsNormal(idx) => dcs_from_table_index(idx, false),
        flash_map::SubaudioCode::DcsInverted(idx) => dcs_from_table_index(idx - 105, true),
    }
}

fn dcs_from_table_index(idx: u16, inverted: bool) -> SubAudio {
    match settings::DCS_TABLE.get((idx as usize).wrapping_sub(1)) {
        Some(&code) => SubAudio::Dcs { code, inverted },
        None => SubAudio::None,
    }
}

pub(super) fn power_from_raw(tx_power: u8) -> Power {
    match tx_power {
        1 => Power::Low,
        2 => Power::Mid,
        _ => Power::High,
    }
}

pub(super) fn power_to_raw(power: Power) -> u8 {
    match power {
        Power::High => 0,
        Power::Low => 1,
        Power::Mid => 2,
    }
}

pub(super) fn modulation_from_raw(raw: u8) -> Modulation {
    match raw {
        1 => Modulation::Am,
        2 => Modulation::Usb,
        3 => Modulation::Cw,
        4 => Modulation::Cwf,
        _ => Modulation::Fm,
    }
}

pub(super) fn modulation_to_raw(modulation: Modulation) -> u8 {
    match modulation {
        Modulation::Fm => 0,
        Modulation::Am => 1,
        Modulation::Usb => 2,
        Modulation::Cw => 3,
        Modulation::Cwf => 4,
    }
}

pub(super) fn digit_value(key: KeyId) -> Option<u8> {
    Some(match key {
        KeyId::Digit0 => 0,
        KeyId::Digit1 => 1,
        KeyId::Digit2 => 2,
        KeyId::Digit3 => 3,
        KeyId::Digit4 => 4,
        KeyId::Digit5 => 5,
        KeyId::Digit6 => 6,
        KeyId::Digit7 => 7,
        KeyId::Digit8 => 8,
        KeyId::Digit9 => 9,
        _ => return None,
    })
}

pub(super) fn subaudio_to_code(sub: SubAudio) -> u16 {
    match sub {
        SubAudio::None => 0,
        SubAudio::Ctcss(t) => t,
        SubAudio::Dcs { code, inverted } => match settings::dcs_index(code) {
            Some(i) => {
                let one_based = i as u16 + 1;
                if inverted {
                    one_based + 105
                } else {
                    one_based
                }
            }
            None => 0,
        },
    }
}

pub(super) const SUBAUDIO_MAX_INDEX: i32 =
    (settings::CTCSS_TABLE.len() + 2 * settings::DCS_TABLE.len() - 1) as i32;

pub(super) fn subaudio_from_index(v: i32) -> SubAudio {
    if v < 0 {
        return SubAudio::None;
    }
    let v = v as usize;
    if v < settings::CTCSS_TABLE.len() {
        return SubAudio::Ctcss(settings::CTCSS_TABLE[v]);
    }
    let dcs_v = v - settings::CTCSS_TABLE.len();
    if dcs_v < settings::DCS_TABLE.len() {
        SubAudio::Dcs {
            code: settings::DCS_TABLE[dcs_v],
            inverted: false,
        }
    } else {
        let inv_v = (dcs_v - settings::DCS_TABLE.len()).min(settings::DCS_TABLE.len() - 1);
        SubAudio::Dcs {
            code: settings::DCS_TABLE[inv_v],
            inverted: true,
        }
    }
}

pub(super) fn subaudio_index(sub: SubAudio) -> i32 {
    match sub {
        SubAudio::None => -1,
        SubAudio::Ctcss(hz) => match settings::ctcss_index(Some(hz)) {
            Some(i) => i as i32,
            None => -1,
        },
        SubAudio::Dcs { code, inverted } => match settings::dcs_index(code) {
            Some(i) => {
                let base = settings::CTCSS_TABLE.len()
                    + if inverted {
                        settings::DCS_TABLE.len()
                    } else {
                        0
                    };
                (base + i) as i32
            }
            None => -1,
        },
    }
}

/// CW's TX chain never writes subaudio registers.
pub(super) fn cw_safe_subaudio(cfg: &ChannelConfig) -> (SubAudio, SubAudio) {
    if cfg.modulation == Modulation::Cw {
        (SubAudio::None, SubAudio::None)
    } else {
        (cfg.subaudio_tx, cfg.subaudio_rx)
    }
}

pub(super) fn map(x: i32, in_min: i32, in_max: i32, out_min: i32, out_max: i32) -> i32 {
    (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min
}

pub(super) fn clamp_step(cur: i32, up: bool, lo: i32, hi: i32) -> i32 {
    if up {
        (cur + 1).min(hi)
    } else {
        (cur - 1).max(lo)
    }
}

pub(super) fn wrap_step(cur: i32, up: bool, lo: i32, hi: i32) -> i32 {
    if up {
        if cur >= hi {
            lo
        } else {
            cur + 1
        }
    } else if cur <= lo {
        hi
    } else {
        cur - 1
    }
}
