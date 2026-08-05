pub mod keys;
pub mod time;
pub mod tracking;

use super::doppler;
use super::input::DigitInput;
use super::name_edit::NameEdit;
use crate::device::radio::Band;
use crate::flash_map::{MAX_SATELLITES, SatRecord};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SatellitePage {
    List,
    Detail,
    TimeSet,
    Tracking,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailField {
    Name,
    RxFreq,
    TxFreq,
    RxTone,
    TxTone,
    Altitude,
    Aos,
    Los,
    MaxEl,
    StartTracking,
    Save,
    Delete,
}

impl DetailField {
    pub const NEW_COUNT: usize = 7;
    pub const EXISTING_COUNT: usize = 12;

    pub fn count(is_new: bool) -> usize {
        if is_new {
            Self::NEW_COUNT
        } else {
            Self::EXISTING_COUNT
        }
    }

    pub fn next(self, is_new: bool) -> Self {
        match self {
            DetailField::Name => DetailField::RxFreq,
            DetailField::RxFreq => DetailField::TxFreq,
            DetailField::TxFreq => DetailField::RxTone,
            DetailField::RxTone => DetailField::TxTone,
            DetailField::TxTone => DetailField::Altitude,
            DetailField::Altitude => {
                if is_new {
                    DetailField::Save
                } else {
                    DetailField::Aos
                }
            }
            DetailField::Aos => DetailField::Los,
            DetailField::Los => DetailField::MaxEl,
            DetailField::MaxEl => DetailField::StartTracking,
            DetailField::StartTracking => DetailField::Save,
            DetailField::Save => {
                if is_new {
                    DetailField::Name
                } else {
                    DetailField::Delete
                }
            }
            DetailField::Delete => DetailField::Name,
        }
    }

    pub fn prev(self, is_new: bool) -> Self {
        match self {
            DetailField::Name => {
                if is_new {
                    DetailField::Save
                } else {
                    DetailField::Delete
                }
            }
            DetailField::RxFreq => DetailField::Name,
            DetailField::TxFreq => DetailField::RxFreq,
            DetailField::RxTone => DetailField::TxFreq,
            DetailField::TxTone => DetailField::RxTone,
            DetailField::Altitude => DetailField::TxTone,
            DetailField::Aos => DetailField::Altitude,
            DetailField::Los => DetailField::Aos,
            DetailField::MaxEl => DetailField::Los,
            DetailField::StartTracking => DetailField::MaxEl,
            DetailField::Save => {
                if is_new {
                    DetailField::Altitude
                } else {
                    DetailField::StartTracking
                }
            }
            DetailField::Delete => DetailField::Save,
        }
    }

    pub fn field_at(index: usize, is_new: bool) -> Self {
        let mut f = DetailField::Name;
        for _ in 0..index {
            f = f.next(is_new);
        }
        f
    }

    pub fn field_index(self, is_new: bool) -> usize {
        let mut f = DetailField::Name;
        for i in 0..=Self::EXISTING_COUNT {
            if f == self {
                return i;
            }
            f = f.next(is_new);
        }
        0 // unreachable for valid fields
    }
}

pub struct SatelliteUi {
    pub(crate) page: SatellitePage,
    pub(crate) list_index: usize,
    pub(crate) detail_index: usize,
    pub(crate) detail_field: DetailField,
    pub(crate) editing: bool,

    pub(crate) name_edit: NameEdit<10>,
    pub(crate) freq_edit: DigitInput<6>,
    pub(crate) time_edit: DigitInput<6>,
    pub(crate) tone_idx: i16,
    pub(crate) tone_snapshot: i16,
    pub(crate) alt_edit: DigitInput<4>,
    pub(crate) el_edit: DigitInput<2>,

    pub(crate) time_h: DigitInput<2>,
    pub(crate) time_m: DigitInput<2>,
    pub(crate) time_s: DigitInput<2>,
    /// Which time field is currently receiving input (0=H, 1=M, 2=S).
    pub(crate) time_field: u8,

    pub(crate) aos_h: u8,
    pub(crate) aos_m: u8,
    pub(crate) aos_s: u8,
    pub(crate) los_h: u8,
    pub(crate) los_m: u8,
    pub(crate) los_s: u8,
    pub(crate) max_el_deg: u16,

    // Tracking state (set when tracking begins)
    pub(crate) active_rx_freq_hz: u32,
    pub(crate) active_tx_freq_hz: u32,
    pub(crate) active_band: Band,
    pub(crate) tracking_sql: u8,
    /// True when SQL is set to "OFF"
    pub(crate) tracking_monitor: bool,
    pub(crate) tracking_wide: bool,
    /// Last computed Doppler shift in Hz.
    pub(crate) current_doppler_hz: i32,

    // Gain menu: 0=none, 1=LNAs, 2=LNA, 3=PGA, 4=IF
    pub(crate) gain_menu: u8,
    // Current gain values read from FD6818 registers
    pub(crate) gain_lnas: u8,
    pub(crate) gain_lna: u8,
    pub(crate) gain_pga: u8,
    pub(crate) gain_if: u16,
    // Pass timing
    pub(crate) pass_aos_abs: u32,
    pub(crate) pass_dur_secs: u32,

    /// True when editing a newly-created (not yet saved) satellite.
    pub(crate) is_new: bool,
    /// True after user has explicitly configured AOS/LOS pass times.
    pub(crate) pass_configured: bool,

    pub(crate) confirm_delete: bool,
}

impl SatelliteUi {
    pub const fn new() -> Self {
        SatelliteUi {
            page: SatellitePage::List,
            list_index: 0,
            detail_index: 0,
            detail_field: DetailField::Name,
            editing: false,
            name_edit: NameEdit::blank(),
            freq_edit: DigitInput::new(),
            time_edit: DigitInput::new(),
            tone_idx: -1,
            tone_snapshot: -1,
            alt_edit: DigitInput::new(),
            el_edit: DigitInput::new(),
            time_h: DigitInput::new(),
            time_m: DigitInput::new(),
            time_s: DigitInput::new(),
            time_field: 0,
            aos_h: 0,
            aos_m: 0,
            aos_s: 0,
            los_h: 0,
            los_m: 0,
            los_s: 0,
            max_el_deg: 45,
            active_rx_freq_hz: 0,
            active_tx_freq_hz: 0,
            active_band: Band::Vhf,
            tracking_sql: 0,
            tracking_monitor: false,
            tracking_wide: true,
            current_doppler_hz: 0,
            gain_menu: 0,
            gain_lnas: 0,
            gain_lna: 0,
            gain_pga: 0,
            gain_if: 0,
            pass_aos_abs: 0,
            pass_dur_secs: 0,
            is_new: false,
            pass_configured: false,
            confirm_delete: false,
        }
    }

    pub fn aos_wall(&self) -> u32 {
        time::wall_secs_from_hms(self.aos_h, self.aos_m, self.aos_s)
    }

    pub fn los_wall(&self) -> u32 {
        time::wall_secs_from_hms(self.los_h, self.los_m, self.los_s)
    }

    pub fn defined_count(sats: &[Option<SatRecord>; MAX_SATELLITES]) -> usize {
        sats.iter().filter(|s| s.is_some()).count()
    }

    /// Index of the first "empty" (undefined) slot, or MAX if full.
    pub fn first_empty_index(sats: &[Option<SatRecord>; MAX_SATELLITES]) -> usize {
        sats.iter().position(|s| s.is_none()).unwrap_or(sats.len())
    }

    pub fn enter_detail(&mut self, idx: usize, is_new: bool) {
        self.detail_index = idx;
        self.detail_field = DetailField::Name;
        self.editing = false;
        self.is_new = is_new;
        self.page = SatellitePage::Detail;
    }

    pub fn enter_time_set(&mut self) {
        self.page = SatellitePage::TimeSet;
        self.time_field = 0; // Start at hours
    }

    /// Begin tracking the satellite at `detail_index`.
    /// Returns the `DopplerState` for the main loop to use.
    /// Returns `None` if the pass times haven't been configured yet.
    pub fn start_tracking(
        &mut self,
        sats: &[Option<SatRecord>; MAX_SATELLITES],
        wall_secs: u32,
    ) -> Option<(doppler::DopplerState, u32, u32, Band)> {
        if !self.pass_configured {
            return None;
        }
        let sat = sats[self.detail_index]?;
        let aos_wall = self.aos_wall();
        let los_wall = self.los_wall();
        let (aos_abs, los_abs) = time::resolve_pass_time(wall_secs, aos_wall, los_wall);

        let band = Band::from_freq_hz(sat.rx_freq_hz);
        let doppler =
            doppler::doppler_init(aos_abs, los_abs, self.max_el_deg, sat.altitude_km, band);

        self.active_rx_freq_hz = sat.rx_freq_hz;
        self.active_tx_freq_hz = sat.tx_freq_hz;
        self.active_band = band;
        self.current_doppler_hz = 0;
        self.pass_aos_abs = doppler.aos_abs;
        self.pass_dur_secs = doppler.dur_sec;

        self.page = SatellitePage::Tracking;

        Some((doppler, sat.rx_freq_hz, sat.tx_freq_hz, band))
    }

    pub fn list_item_count(sats: &[Option<SatRecord>; MAX_SATELLITES]) -> usize {
        let count = Self::defined_count(sats);
        if count < MAX_SATELLITES {
            1 + count + 1 // Set Time + satellites + Add
        } else {
            1 + count // Set Time + satellites (full)
        }
    }

    /// Map list_index to the actual satellite flash index
    pub fn sat_index_for_list_pos(
        &self,
        sats: &[Option<SatRecord>; MAX_SATELLITES],
    ) -> Option<usize> {
        let pos = self.list_index;
        if pos == 0 {
            return None; // Set Time header
        }
        let sat_pos = pos - 1;
        let mut found = 0;
        for (i, slot) in sats.iter().enumerate() {
            if slot.is_some() {
                if found == sat_pos {
                    return Some(i);
                }
                found += 1;
            }
        }
        // "Add Satellite" row
        None
    }

    /// Navigate list up (wrapping).
    pub fn list_up(&mut self, sats: &[Option<SatRecord>; MAX_SATELLITES]) {
        let total = Self::list_item_count(sats);
        if total == 0 {
            return;
        }
        if self.list_index == 0 {
            self.list_index = total - 1;
        } else {
            self.list_index -= 1;
        }
    }

    /// Navigate list down (wrapping).
    pub fn list_down(&mut self, sats: &[Option<SatRecord>; MAX_SATELLITES]) {
        let total = Self::list_item_count(sats);
        if total == 0 {
            return;
        }
        self.list_index += 1;
        if self.list_index >= total {
            self.list_index = 0;
        }
    }
}

pub(super) fn poll_name_timeout(ui: &mut SatelliteUi) {
    if ui.page == SatellitePage::Detail && ui.editing && ui.detail_field == DetailField::Name {
        ui.name_edit.tick();
    }
}
