mod aprs_ops;
mod ax25;
mod chanmgr;
mod contacts;
mod convert;
mod fm;
mod input;
mod keyfn;
mod keys;
mod launcher;
mod name_edit;
mod scan;
mod scanqt;
mod search;
mod settings;
mod settings_ops;
mod side;
mod side_ops;
mod spectrum;
mod tx;

use convert::*;
use input::*;

use crate::device::flashlight::Flashlight;
use crate::device::fm_radio::FmRadio;
use crate::device::keypad::Keypad;
use crate::device::radio::{
    BandLock, ChannelConfig, Modulation, Power, Radio, RogerTone, SubAudio,
};
use crate::device::storage::Storage;
use crate::flash_map::{self, addr};
use cortex_m::peripheral::SYST;

const FIRMWARE_VERSION: &str = env!("GIT_VERSION");

const STEP_LIST_DECI_HZ: [u32; 9] = [250, 500, 625, 1000, 1250, 2000, 2500, 5000, 10000];
const DEFAULT_STEP_INDEX: u8 = 3;

/// Highest valid channel index. `0..=999`, 1000 total slots
const MAX_CHANNEL_NUM: u16 = 999;

const VFO_INPUT_DIGITS: usize = 6;
const CHANNEL_INPUT_DIGITS: usize = 3;

const DUAL_STANDBY_HOLD_TICKS: u16 = 10;
const TICKS_PER_SECOND: u16 = 100;

const ANI_DISPLAY_HOLD_TICKS: u16 = 500;

const NO_CHANNELS_HOLD_TICKS: u16 = 200;

const RX_BLINK_HALF_PERIOD: u16 = 5;

const DTMF_DIAL_MAX_DIGITS: usize = 16;

const POWER_SAVE_IDLE_TICKS: u16 = 1000;
const POWER_SAVE_AWAKE_TICKS: u16 = 10;
const POWER_SAVE_SLEEP_TICKS_PER_LEVEL: u16 = 10;

const BACKLIGHT_STEP_TICKS: u16 = 500;

/// TODO: calibrate per band
const RSSI_DBM_BASE: i16 = 160 - 6 + 20;

pub fn rssi_raw_to_dbm(raw: u16) -> i32 {
    raw as i32 - RSSI_DBM_BASE as i32
}

pub fn dbm_to_rssi_raw(dbm: i16) -> i32 {
    dbm as i32 + RSSI_DBM_BASE as i32
}

/// Fixed reference point for `App::battery_voltage_cv`'s linear calibration
/// (hundredths of a volt): `battery_cal_raw` is the raw ADC reading that's
/// supposed to correspond to exactly this voltage, tuned in the field
/// against a multimeter.
pub const BATTERY_CAL_REFERENCE_CV: u16 = 760;

const VOX_THRESHOLD_TABLE: [u8; 11] = [127, 52, 62, 72, 84, 95, 106, 117, 125, 132, 140];
const VOX_TX_HYSTERESIS: u8 = 8;
const VOX_HOLD_AFTER_RX_TICKS: u8 = 150;
const VOX_HOLD_AFTER_KEY_TICKS: u8 = 40;
const VOX_HOLD_AFTER_TX_FAIL_TICKS: u8 = 120;
const VOX_HOLD_AFTER_PTT_TICKS: u8 = 100;

const RTONE_HZ_DIV_10: [u16; 4] = [100, 145, 175, 210];

/// CW/CWF sidetone pitch: a common CW monitor-tone frequency.
const CW_SIDETONE_HZ_DIV_10: u16 = 70;
/// BK-IN=Semi hang time after the last key release before dropping back to
/// RX, in `TICKS_PER_SECOND` units.
const CW_SEMI_HANG_TICKS: u16 = 80;
/// BK-IN=Full's equivalent: much shorter
const CW_FULL_HANG_TICKS: u16 = 6;

// Mode
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standby,
    AppMenu,
    Settings,
    ChanMgr,
    Fm,
    Scan,
    Search,
    ScanQt,
    Contacts,
    Spectrum,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChVfoMode {
    Vfo,
    Channel,
}

/// Search mode's coarse status, for the standby-adjacent UI screens (the
/// full state machine lives in `search`, private to this module).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    Hunting,
    Listening,
    Found,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelDisplayMode {
    Frequency,
    Name,
    NameFreq,
}

pub struct App {
    pub radio: Radio,
    keypad: Keypad,
    storage: Storage,
    flashlight: Flashlight,
    mode: Mode,
    sides: [side::Side; 2],
    settings: flash_map::Settings,
    settings_ui: settings::SettingsUi,
    chanmgr: chanmgr::ChanMgrUi,
    contacts: contacts::ContactsUi,
    launcher_index: usize,

    master: usize,
    watching: usize,
    last_signal_side: Option<usize>,

    input: InputBuf,
    key_lock: bool,
    transmitting: bool,
    tx_prohibited: bool,
    /// APRS beacon TX in progress, set by `send_beacon`, handled by main loop.
    aprs_beacon_pending: bool,
    /// NRZI-encoded bitstream for the pending APRS beacon.
    aprs_beacon_nrzi: [u8; 512],
    /// Number of valid bits in `aprs_beacon_nrzi`.
    aprs_beacon_nrzi_bits: usize,
    dual_standby: bool,
    dual_hold_ticks: u16,
    tot_ticks: u16,

    key_idle_ticks: u16,
    vox_det_dly: u8,
    vox_work_dly: u8,
    vox_active: bool,
    rtone_sounding: bool,
    rtone_self_keyed: bool,

    /// Electric key state while the master side's modulation is `Cw`/`Cwf`;
    /// `set_ptt` routes to `set_cw_key` instead of the normal voice-PTT
    /// path in that case.
    cw_key_down: bool,
    /// Whether a CW/CWF carrier is currently keyed up, independent of
    /// `transmitting` (which is voice-PTT specific).
    cw_tx_active: bool,
    /// BK-IN=Semi hang countdown after the last key release; `poll_cw_hang`
    /// ticks it down and drops back to RX at zero.
    cw_hang_ticks: u16,

    ani_caller: Option<[u8; 3]>,
    ani_hold_ticks: u16,
    /// Ticks remaining to show the "NO CHANNELS" overlay after a VFO/Channel

    /// toggle finds nothing programmed. 0 = not showing.
    no_channels_ticks: u16,
    blink_phase: u16,
    /// Runtime "current call target" override:
    /// sticky across PTTs, set by picking a contact in the Contacts app or
    /// by a 3-digit manual DTMF dial, cleared whenever the active channel
    /// changes. Falls back to the side's own `ani_target` (a contact-table
    /// index) when `None`.
    ani_target_override: Option<[u8; 3]>,
    /// `Some` while the standby DTMF dial input box (`*` key) is open;
    /// `None` the rest of the time (normal standby display).
    dtmf_dial: Option<DigitInput<DTMF_DIAL_MAX_DIGITS>>,

    battery_cal: [u8; 7],
    battery_bars: u8,
    /// Latest raw 12-bit battery-ADC sample, refreshed by `poll_battery`.
    /// Kept separately from `battery_bars` (which uses the factory 8-bit
    /// threshold table) since `battery_voltage_cv` needs full precision.
    battery_raw12: u16,

    rssi_raw: u8,
    mic_level: u8,

    channel_display_mode: ChannelDisplayMode,

    power_save: bool,
    ps_asleep: bool,
    ps_idle_ticks: u16,
    ps_cycle_ticks: u16,

    bl_idle_ticks: u16,

    scan: scan::ScanState,
    search: search::SearchState,
    scanqt: scanqt::ScanQtState,

    fm: fm::FmState,
    fm_radio: FmRadio<'static>,
    fm_channels: [u16; flash_map::FM_CHANNEL_COUNT],

    spectrum: spectrum::SpectrumState,
}

impl App {
    pub fn new(
        mut radio: Radio,
        keypad: Keypad,
        mut storage: Storage,
        flashlight: Flashlight,
        fm_radio: FmRadio<'static>,
        default_cfg: ChannelConfig,
        syst: &mut SYST,
    ) -> Self {
        let vfo_payload = storage.load_vfo_raw();
        let settings = storage
            .load_settings()
            .unwrap_or(flash_map::Settings::DEFAULT);
        let battery_cal = storage.read_battery_calibration();
        let fm_channels = storage
            .load_fm_channels()
            .unwrap_or([flash_map::FM_CHANNEL_EMPTY; flash_map::FM_CHANNEL_COUNT]);

        let mut sides = [
            side::Side {
                vfo_chan: ChVfoMode::Vfo,
                channel_num: 0,
                freq_step: DEFAULT_STEP_INDEX,
                rx_freq_hz: default_cfg.freq_hz,
                tx_freq_hz: default_cfg.tx_freq_hz,
                freq_dir: 0,
                offset_hz: 0,
                reversed: false,
                ani_target: None,
                cfg: default_cfg,
                name: [0; 12],
                vfo_backup: side::VfoBackup {
                    rx_freq_hz: default_cfg.freq_hz,
                    freq_dir: 0,
                    offset_hz: 0,
                    wide_band: default_cfg.wide_band,
                    power: default_cfg.power,
                    subaudio_tx: default_cfg.subaudio_tx,
                    subaudio_rx: default_cfg.subaudio_rx,
                    ani_target: None,
                },
            },
            side::Side {
                vfo_chan: ChVfoMode::Vfo,
                channel_num: 0,
                freq_step: DEFAULT_STEP_INDEX,
                rx_freq_hz: default_cfg.freq_hz,
                tx_freq_hz: default_cfg.tx_freq_hz,
                freq_dir: 0,
                offset_hz: 0,
                reversed: false,
                ani_target: None,
                cfg: default_cfg,
                name: [0; 12],
                vfo_backup: side::VfoBackup {
                    rx_freq_hz: default_cfg.freq_hz,
                    freq_dir: 0,
                    offset_hz: 0,
                    wide_band: default_cfg.wide_band,
                    power: default_cfg.power,
                    subaudio_tx: default_cfg.subaudio_tx,
                    subaudio_rx: default_cfg.subaudio_rx,
                    ani_target: None,
                },
            },
        ];

        if let Some(buf) = vfo_payload {
            for (half, s) in sides.iter_mut().enumerate() {
                let bytes: [u8; addr::VFO_SIZE as usize] = buf
                    [half * addr::VFO_SIZE as usize..(half + 1) * addr::VFO_SIZE as usize]
                    .try_into()
                    .unwrap();
                let vfo = flash_map::VfoMode::from_bytes(&bytes);
                if vfo.freq_deci_hz() != 0 {
                    s.load_vfo(&vfo);
                }
            }
        }

        // Restore each side's last active mode (VFO vs Channel) + modulation
        if let Some(state) = storage.load_channel_state() {
            for (half, s) in sides.iter_mut().enumerate() {
                let is_channel = state[half * 3] != 0;
                let num = u16::from_le_bytes([state[half * 3 + 1], state[half * 3 + 2]])
                    .min(MAX_CHANNEL_NUM);
                if is_channel && !storage.is_channel_empty(num) {
                    let ch = storage.read_channel(num);
                    s.load_channel(num, &ch);
                }
                s.cfg.modulation = modulation_from_raw(state[6 + half]);
            }
        }

        radio.set_frequency(sides[0].cfg.freq_hz);
        radio.set_tx_frequency(sides[0].cfg.tx_freq_hz);
        radio.set_power(sides[0].cfg.power);
        let (boot_subaudio_tx, boot_subaudio_rx) = cw_safe_subaudio(&sides[0].cfg);
        radio.set_subaudio_tx(boot_subaudio_tx);
        radio.set_subaudio_rx(boot_subaudio_rx);
        radio.set_modulation(sides[0].cfg.modulation);
        radio.set_sql_level(syst, settings.sql_level);
        radio.set_tail_elimination(settings.tail_elimination);
        radio.set_rptrl(settings.rptrl);
        radio.set_beeps_enabled(settings.beeps_switch);
        radio.set_roger_tone(RogerTone::from_u8(settings.roger_tone));
        radio.set_scramble_level(syst, settings.scramble_level);
        radio.set_rit_offset(settings.rit_offset as i32 * 10);
        radio.set_tx_allowed(BandLock::from_u8(settings.band_lock).tx_ranges());

        App {
            radio,
            keypad,
            storage,
            mode: Mode::Standby,
            sides,
            settings,
            settings_ui: settings::SettingsUi::new(),
            chanmgr: chanmgr::ChanMgrUi::new(),
            contacts: contacts::ContactsUi::new(),
            launcher_index: 0,
            master: 0,
            watching: 0,
            last_signal_side: None,
            input: InputBuf::new(),
            key_lock: false,
            transmitting: false,
            tx_prohibited: false,
            aprs_beacon_pending: false,
            aprs_beacon_nrzi: [0u8; 512],
            aprs_beacon_nrzi_bits: 0,
            flashlight,
            dual_standby: settings.dual_standby,
            dual_hold_ticks: DUAL_STANDBY_HOLD_TICKS,
            tot_ticks: 0,
            key_idle_ticks: 0,
            vox_det_dly: 0,
            vox_work_dly: 0,
            vox_active: false,
            rtone_sounding: false,
            rtone_self_keyed: false,
            cw_key_down: false,
            cw_tx_active: false,
            cw_hang_ticks: 0,
            ani_caller: None,
            ani_hold_ticks: 0,
            no_channels_ticks: 0,
            blink_phase: 0,
            ani_target_override: None,
            dtmf_dial: None,
            battery_cal,
            battery_bars: 4,
            battery_raw12: 0,
            rssi_raw: 0,
            mic_level: 0,
            channel_display_mode: channel_display_mode_from_u8(settings.channel_display_mode),
            power_save: false,
            ps_asleep: false,
            ps_idle_ticks: POWER_SAVE_IDLE_TICKS,
            ps_cycle_ticks: 0,
            bl_idle_ticks: settings.backlight_time as u16 * BACKLIGHT_STEP_TICKS,
            scan: scan::ScanState::new(),
            search: search::SearchState::new(),
            scanqt: scanqt::ScanQtState::new(),
            fm: fm::FmState::new(),
            fm_radio,
            fm_channels,
            spectrum: spectrum::SpectrumState::new(default_cfg.freq_hz),
        }
    }

    // dual standby
    pub fn set_dual_standby(&mut self, syst: &mut SYST, enabled: bool) {
        self.dual_standby = enabled;
        self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;

        // force radio to watch current master side
        if !enabled && self.watching != self.master {
            self.watching = self.master;
            self.apply_watching_to_radio(syst);
        }
    }

    pub fn poll_dual_standby(&mut self, syst: &mut SYST, signal_present: bool) {
        if signal_present {
            self.last_signal_side = Some(self.watching);
        }
        if !self.dual_standby || self.mode != Mode::Standby || self.transmitting {
            return;
        }
        if self.power_save {
            return;
        }
        if self.radio.is_monitor() {
            if self.watching != self.master {
                self.watching = self.master;
                self.apply_watching_to_radio(syst);
            }
            self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
            return;
        }
        if signal_present {
            self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
            return;
        }
        if self.dual_hold_ticks > 0 {
            self.dual_hold_ticks -= 1;
            return;
        }
        self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
        self.watching = 1 - self.watching;
        self.apply_watching_to_radio(syst);
    }

    fn note_power_save_activity(&mut self, syst: &mut SYST) {
        if self.ps_asleep {
            self.radio.enter_rx(syst);
            self.ps_asleep = false;
        }
        self.power_save = false;
        self.ps_idle_ticks = POWER_SAVE_IDLE_TICKS;
        self.ps_cycle_ticks = 0;
    }

    fn reset_power_save(&mut self, syst: &mut SYST) {
        self.note_power_save_activity(syst);
    }

    pub fn power_save_is_asleep(&self) -> bool {
        self.ps_asleep
    }

    pub fn poll_power_save(&mut self, syst: &mut SYST) {
        if self.settings.save_level == 0 || self.mode != Mode::Standby || self.transmitting {
            if self.ps_idle_ticks != POWER_SAVE_IDLE_TICKS || self.ps_asleep {
                self.note_power_save_activity(syst);
            }
            return;
        }

        if !self.ps_asleep && (self.radio.rssi_open() || self.radio.audio_is_open()) {
            self.power_save = false;
            self.ps_idle_ticks = POWER_SAVE_IDLE_TICKS;
            self.ps_cycle_ticks = 0;
            return;
        }

        if self.ps_idle_ticks > 0 {
            self.ps_idle_ticks -= 1;
            return;
        }

        self.power_save = true;
        if self.ps_cycle_ticks > 0 {
            self.ps_cycle_ticks -= 1;
            return;
        }

        if self.ps_asleep {
            self.radio.enter_rx(syst);
            self.ps_asleep = false;
            self.ps_cycle_ticks = POWER_SAVE_AWAKE_TICKS;
        } else {
            // About to sleep: if dual-standby is on, hand the *next* wake
            // to the other side. Only the cached config is pushed here,
            // the side we just woke up on already got its fair squelch
            // check during the awake window that's ending now.
            if self.dual_standby {
                self.watching = 1 - self.watching;
                self.push_watching_config();
            }
            self.radio.rf_sleep(syst);
            self.ps_asleep = true;
            self.ps_cycle_ticks =
                self.settings.save_level as u16 * POWER_SAVE_SLEEP_TICKS_PER_LEVEL;
        }
    }

    // backlight
    fn note_backlight_activity(&mut self) {
        self.bl_idle_ticks = self.settings.backlight_time as u16 * BACKLIGHT_STEP_TICKS;
    }

    pub fn poll_backlight(&mut self) {
        if self.radio.audio_is_open() {
            self.note_backlight_activity();
        }
        if self.bl_idle_ticks > 0 {
            self.bl_idle_ticks -= 1;
        }
    }

    pub fn backlight_should_be_on(&self) -> bool {
        self.transmitting || self.settings.backlight_time == 0 || self.bl_idle_ticks > 0
    }

    // persistence
    fn save_vfo(&mut self) {
        let mut buf = [0u8; 64];
        for (half, s) in self.sides.iter().enumerate() {
            buf[half * 32..(half + 1) * 32].copy_from_slice(&s.to_vfo_bytes());
        }
        self.storage.save_vfo_raw(&buf);
    }

    fn save_settings(&mut self) {
        self.storage.save_settings(&self.settings);
    }

    pub fn save_channel_state(&mut self) {
        let mut buf = [0u8; 8];
        for (half, s) in self.sides.iter().enumerate() {
            buf[half * 3] = matches!(s.vfo_chan, ChVfoMode::Channel) as u8;
            let bytes = s.channel_num.to_le_bytes();
            buf[half * 3 + 1] = bytes[0];
            buf[half * 3 + 2] = bytes[1];
            buf[6 + half] = modulation_to_raw(s.cfg.modulation);
        }
        self.storage.save_channel_state(&buf);
    }

    pub(super) fn refresh_channel_display(&mut self, num: u16) {
        side_ops::refresh_channel_display(self, num);
    }

    fn load_channel_num(&mut self, num: u16) {
        side_ops::load_channel_num(self, num);
    }

    fn sync_watching_to_master(&mut self, syst: &mut SYST) {
        side_ops::sync_watching_to_master(self, syst);
    }

    // radio sync
    fn push_watching_config(&mut self) {
        side_ops::push_watching_config(self);
    }

    fn apply_watching_to_radio(&mut self, syst: &mut SYST) {
        side_ops::apply_watching_to_radio(self, syst);
    }

    // frequency / channel stepping
    fn step(&mut self, syst: &mut SYST, up: bool) {
        side_ops::step(self, syst, up);
    }

    fn commit_input(&mut self, syst: &mut SYST) {
        side_ops::commit_input(self, syst);
    }

    fn toggle_vfo_channel(&mut self, syst: &mut SYST) {
        side_ops::toggle_vfo_channel(self, syst);
    }

    fn switch_side(&mut self, syst: &mut SYST) {
        side_ops::switch_side(self, syst);
    }

    fn toggle_reverse(&mut self, syst: &mut SYST) {
        side_ops::toggle_reverse(self, syst);
    }

    fn toggle_power(&mut self, syst: &mut SYST) {
        side_ops::toggle_power(self, syst);
    }

    fn toggle_wide_narrow(&mut self, syst: &mut SYST) {
        side_ops::toggle_wide_narrow(self, syst);
    }

    fn toggle_modulation(&mut self, syst: &mut SYST) {
        side_ops::toggle_modulation(self, syst);
    }

    fn commit_side_change(&mut self, syst: &mut SYST) {
        side_ops::commit_side_change(self, syst);
    }

    // auto-lock
    fn reset_key_idle(&mut self) {
        self.key_idle_ticks = self.settings.key_auto_lock as u16 * 50;
    }

    pub fn poll_auto_lock(&mut self, syst: &mut SYST, rx_active: bool) {
        if self.settings.key_auto_lock == 0
            || self.key_lock
            || self.mode != Mode::Standby
            || self.transmitting
            || rx_active
        {
            return;
        }
        if self.key_idle_ticks > 0 {
            self.key_idle_ticks -= 1;
            if self.key_idle_ticks == 0 {
                self.key_lock = true;
                self.radio.play_beep(syst);
            }
        }
    }

    // VOX
    pub fn poll_vox(&mut self, syst: &mut SYST, mic_level: u8, rx_active: bool) {
        if self.vox_det_dly > 0 {
            self.vox_det_dly -= 1;
        }
        if self.vox_work_dly > 0 {
            self.vox_work_dly -= 1;
        }
        if !self.settings.vox_switch {
            return;
        }
        if self.mode != Mode::Standby || (!self.transmitting && rx_active) {
            self.vox_det_dly = VOX_HOLD_AFTER_RX_TICKS;
            return;
        }
        if self.vox_det_dly != 0 {
            return;
        }

        let level = self.settings.vox_level.clamp(1, 9) as usize;
        let mut threshold = VOX_THRESHOLD_TABLE[level];
        if self.transmitting {
            threshold = threshold.saturating_sub(VOX_TX_HYSTERESIS);
        }

        if mic_level > threshold {
            self.vox_work_dly = 50 + self.settings.vox_delay.min(15) * 10;
            if !self.transmitting {
                self.set_ptt(syst, true);
                if self.transmitting {
                    self.vox_active = true;
                } else {
                    self.vox_det_dly = VOX_HOLD_AFTER_TX_FAIL_TICKS;
                    self.vox_work_dly = 0;
                }
            }
        }
        if self.vox_active && self.vox_work_dly == 0 {
            self.vox_active = false;
            self.set_ptt(syst, false);
        }
    }

    // battery
    pub fn poll_battery(&mut self) {
        let raw12 = self.radio.read_battery_raw();
        self.battery_raw12 = raw12;
        let raw = (raw12 >> 4) as u8;
        let t = &self.battery_cal;
        self.battery_bars = if raw > t[5] {
            4
        } else if raw > t[4] {
            3
        } else if raw > t[3] {
            2
        } else if raw > t[2] {
            1
        } else {
            0
        };
    }

    pub fn battery_bars(&self) -> u8 {
        self.battery_bars
    }

    /// Battery voltage in hundredths of a volt, derived from the latest
    /// raw ADC sample and the field-tuned `battery_cal_raw` calibration point
    pub fn battery_voltage_cv(&self) -> u16 {
        (self.battery_raw12 as u32 * BATTERY_CAL_REFERENCE_CV as u32
            / self.settings.battery_cal_raw.max(1) as u32) as u16
    }

    pub fn settings(&self) -> &flash_map::Settings {
        &self.settings
    }

    pub fn set_rssi_raw(&mut self, val: u8) {
        self.rssi_raw = val;
    }

    pub fn rssi_dbm(&self) -> i32 {
        rssi_raw_to_dbm(self.rssi_raw as u16)
    }

    pub fn s_meter_level(&self) -> u8 {
        let pos = (-self.rssi_dbm()).clamp(53, 141);

        if pos >= 93 {
            map(pos, 141, 93, 1, 9).clamp(1, 9) as u8
        } else {
            let over = map(pos, 93, 53, 0, 4).clamp(0, 4);
            (9 + over) as u8
        }
    }

    pub fn s_meter_s_number(&self) -> u8 {
        let pos = (-self.rssi_dbm()).clamp(53, 141);
        map(pos, 141, 93, 1, 9).clamp(1, 9) as u8
    }

    /// How many dB over the S9 reference (-93dBm, matching f4hwn's
    /// `rssi_dBm >= 93` threshold in its own pos-space) the signal reads,
    /// 0..40, at full 1dB precision -- not bucketed into 10dB bar
    /// segments like `s_meter_level`. Rounding to the nearest bar segment
    /// made two radios reading the same signal within ~2dB of each other
    /// (K6 -57dBm vs. a real BK4819 radio at -59dBm) show wildly
    /// different labels (S9+30 vs. S9+35) even though the underlying
    /// measurements agreed closely.
    pub fn s_meter_over_s9_dbm(&self) -> i32 {
        let pos = (-self.rssi_dbm()).clamp(53, 141);
        (93 - pos).max(0)
    }

    pub fn mic_level(&self) -> u8 {
        self.mic_level
    }

    pub fn set_mic_level(&mut self, level: u8) {
        self.mic_level = level;
    }

    // DTMF
    pub fn poll_dtmf(&mut self, syst: &mut SYST) {
        if let Some(caller) = self.radio.poll_dtmf(syst) {
            self.ani_caller = Some(caller);
            self.ani_hold_ticks = ANI_DISPLAY_HOLD_TICKS;
        } else if self.ani_hold_ticks > 0 {
            self.ani_hold_ticks -= 1;
            if self.ani_hold_ticks == 0 {
                self.ani_caller = None;
            }
        }
    }

    pub fn ani_caller_id(&self) -> Option<[u8; 3]> {
        self.ani_caller
    }

    pub fn poll_no_channels_notice(&mut self) {
        if self.no_channels_ticks > 0 {
            self.no_channels_ticks -= 1;
        }
    }

    pub fn no_channels_notice(&self) -> bool {
        self.no_channels_ticks > 0
    }

    pub fn poll_blink(&mut self) {
        self.blink_phase = self.blink_phase.wrapping_add(1);
    }

    pub fn rx_blink_on(&self) -> bool {
        (self.blink_phase / RX_BLINK_HALF_PERIOD).is_multiple_of(2)
    }

    // key dispatch
    pub fn poll_keys(&mut self, syst: &mut SYST) {
        self.keypad.poll(syst);
        while let Some(ev) = self.keypad.pop_event() {
            keys::dispatch(self, syst, ev);
        }
    }

    pub fn launcher_index(&self) -> usize {
        self.launcher_index
    }

    pub fn launcher_item_count(&self) -> usize {
        launcher::LAUNCHER_ITEMS.len()
    }

    pub fn launcher_label_at(&self, index: usize) -> &'static str {
        launcher::LAUNCHER_ITEMS[index].label()
    }

    pub fn launcher_available_at(&self, index: usize) -> bool {
        launcher::LAUNCHER_ITEMS[index].is_available()
    }

    // settings UI
    /// Whether to draw the up/down arrow chrome around the selected row's
    /// value: true for a cycled Settings item, false while it's in
    /// text-entry mode
    pub fn settings_show_arrows(&self) -> bool {
        self.settings_ui.editing
            && settings::SETTINGS_ORDER[self.settings_ui.index] != settings::SettingItem::Offse
            && settings::SETTINGS_ORDER[self.settings_ui.index] != settings::SettingItem::AprsCall
            && settings::SETTINGS_ORDER[self.settings_ui.index]
                != settings::SettingItem::AprsDevName
            && settings::SETTINGS_ORDER[self.settings_ui.index]
                != settings::SettingItem::AprsComment
            && settings::SETTINGS_ORDER[self.settings_ui.index] != settings::SettingItem::AprsFreq
            && settings::SETTINGS_ORDER[self.settings_ui.index] != settings::SettingItem::AprsLat
            && settings::SETTINGS_ORDER[self.settings_ui.index] != settings::SettingItem::AprsLon
    }

    pub fn settings_index(&self) -> usize {
        self.settings_ui.index
    }

    pub fn settings_item_count(&self) -> usize {
        settings::SETTINGS_ORDER.len()
    }

    pub fn settings_label_at(&self, index: usize) -> &'static str {
        settings::SETTINGS_ORDER[index].label()
    }

    pub fn settings_suppress_label(&self, index: usize) -> bool {
        self.settings_ui.is_editing(index)
            && matches!(
                settings::SETTINGS_ORDER[index],
                settings::SettingItem::AprsComment
            )
    }

    pub fn settings_value_at(&self, index: usize, w: &mut dyn core::fmt::Write) {
        settings_ops::value_text_for(self, index, settings::SETTINGS_ORDER[index], w)
    }

    pub fn settings_cursor(&self, index: usize) -> Option<usize> {
        if !self.settings_ui.is_editing(index) {
            return None;
        }
        match settings::SETTINGS_ORDER[index] {
            settings::SettingItem::AprsCall => Some(self.settings_ui.aprs_call_edit.cursor),
            settings::SettingItem::AprsDevName => Some(self.settings_ui.aprs_dev_name_edit.cursor),
            settings::SettingItem::AprsComment => Some(self.settings_ui.aprs_comment_edit.cursor),
            _ => None,
        }
    }

    // channel manager UI
    pub fn chanmgr_is_detail(&self) -> bool {
        chanmgr::is_detail(self)
    }
    pub fn chanmgr_list_row_count(&self) -> usize {
        chanmgr::list_row_count(self)
    }
    pub fn chanmgr_list_selected_index(&self) -> usize {
        chanmgr::list_selected_index(self)
    }
    /// Reads flash for the currently-visible rows' channel names, so this
    /// needs `&mut self`.
    pub fn chanmgr_list_label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        chanmgr::list_label(self, index, w)
    }
    pub fn chanmgr_field_count(&self) -> usize {
        chanmgr::detail_field_count(self)
    }
    pub fn chanmgr_field_index(&self) -> usize {
        chanmgr::detail_field_index(self)
    }
    /// Whether to draw the up/down arrow chrome: true only for a field that
    /// actually cycles via `Up`/`Down`, false for the text-entry fields
    pub fn chanmgr_show_arrows(&self) -> bool {
        chanmgr::detail_show_arrows(self)
    }
    pub fn chanmgr_field_label(&self, index: usize, w: &mut dyn core::fmt::Write) {
        chanmgr::detail_label(self, index, w)
    }
    pub fn chanmgr_field_value(&self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        chanmgr::detail_value(self, index, w)
    }
    pub fn chanmgr_field_cursor(&self, index: usize) -> Option<usize> {
        chanmgr::detail_cursor(self, index)
    }
    pub fn chanmgr_detail_title(&self, w: &mut dyn core::fmt::Write) {
        chanmgr::detail_title(self, w)
    }
    pub fn poll_chanmgr_name_timeout(&mut self) {
        chanmgr::poll_name_timeout(self);
    }

    pub fn poll_aprs_call_timeout(&mut self) {
        if self.mode == Mode::Settings
            && self.settings_ui.editing
            && settings::SETTINGS_ORDER[self.settings_ui.index] == settings::SettingItem::AprsCall
        {
            self.settings_ui.aprs_call_edit.tick();
        }
    }

    pub fn poll_aprs_dev_name_timeout(&mut self) {
        if self.mode == Mode::Settings
            && self.settings_ui.editing
            && settings::SETTINGS_ORDER[self.settings_ui.index]
                == settings::SettingItem::AprsDevName
        {
            self.settings_ui.aprs_dev_name_edit.tick();
        }
    }

    pub fn poll_aprs_comment_timeout(&mut self) {
        if self.mode == Mode::Settings
            && self.settings_ui.editing
            && settings::SETTINGS_ORDER[self.settings_ui.index]
                == settings::SettingItem::AprsComment
        {
            self.settings_ui.aprs_comment_edit.tick();
        }
    }

    // contacts app UI
    pub fn contacts_is_detail(&self) -> bool {
        contacts::is_detail(self)
    }
    pub fn contacts_list_row_count(&self) -> usize {
        contacts::list_row_count()
    }
    pub fn contacts_list_selected_index(&self) -> usize {
        contacts::list_selected_index(self)
    }
    pub fn contacts_list_label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        contacts::list_label(self, index, w)
    }
    pub fn contacts_field_count(&self) -> usize {
        contacts::detail_field_count(self)
    }
    pub fn contacts_field_index(&self) -> usize {
        contacts::detail_field_index(self)
    }
    pub fn contacts_field_label(&self, index: usize, w: &mut dyn core::fmt::Write) {
        contacts::detail_label(self, index, w)
    }
    pub fn contacts_field_value(&self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        contacts::detail_value(self, index, w)
    }
    pub fn contacts_field_cursor(&self, index: usize) -> Option<usize> {
        contacts::detail_cursor(self, index)
    }
    pub fn contacts_detail_title(&self, w: &mut dyn core::fmt::Write) {
        contacts::detail_title(self, w)
    }
    pub fn poll_contacts_name_timeout(&mut self) {
        contacts::poll_name_timeout(self);
    }

    fn set_ani_target_override(&mut self, id: [u8; 3]) {
        tx::set_ani_target_override(self, id);
    }

    // PTT
    pub fn set_ptt(&mut self, syst: &mut SYST, pressed: bool) {
        tx::set_ptt(self, syst, pressed);
    }

    // --- CW / TOT poll delegates ------------------------------------------

    pub fn poll_cw_key(&mut self, syst: &mut SYST) {
        tx::poll_cw_key(self, syst);
    }

    pub fn poll_cw_hang(&mut self, syst: &mut SYST) {
        tx::poll_cw_hang(self, syst);
    }

    pub fn poll_tot(&mut self, syst: &mut SYST) {
        tx::poll_tot(self, syst);
    }

    // getters
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn is_transmitting(&self) -> bool {
        self.transmitting || self.cw_tx_active
    }
    pub fn tx_prohibited(&self) -> bool {
        self.tx_prohibited
    }

    pub fn aprs_beacon_active(&self) -> bool {
        self.aprs_beacon_pending
    }

    pub(super) fn set_aprs_beacon_pending(&mut self, nrzi: [u8; 512], bits: usize) {
        self.aprs_beacon_nrzi = nrzi;
        self.aprs_beacon_nrzi_bits = bits;
        self.aprs_beacon_pending = true;
    }

    pub fn transmit_pending_aprs_beacon(&mut self, syst: &mut SYST) {
        use crate::device::radio::Power;

        let tone_1200: u16 = (1200u32 * 1_032_444 / 100_000) as u16;
        let tone_2200: u16 = (2200u32 * 1_032_444 / 100_000) as u16;
        let bits = self.aprs_beacon_nrzi_bits;
        let power = match self.settings.aprs_power {
            1 => Power::Mid,
            2 => Power::High,
            _ => Power::Low,
        };

        self.radio.transmit_afsk(
            syst,
            &self.aprs_beacon_nrzi,
            bits,
            self.settings.aprs_freq_hz,
            power,
            tone_1200,
            tone_2200,
            300, // TX_DELAY_MS
            50,  // TX_TAIL_MS
        );
        self.radio.enter_rx(syst);
        self.aprs_beacon_pending = false;
    }
    pub fn is_key_locked(&self) -> bool {
        self.key_lock
    }
    pub fn contrast(&self) -> u8 {
        self.settings.contrast
    }
    pub fn vox_enabled(&self) -> bool {
        self.settings.vox_switch
    }
    pub fn master_index(&self) -> usize {
        self.master
    }
    pub fn master_freq_hz(&self) -> u32 {
        self.sides[self.master].cfg.freq_hz
    }
    pub fn watching_freq_hz(&self) -> u32 {
        self.sides[self.watching].cfg.freq_hz
    }
    pub fn watching_channel_num(&self) -> u16 {
        self.sides[self.watching].channel_num
    }
    pub fn watching_is_channel_mode(&self) -> bool {
        self.sides[self.watching].vfo_chan == ChVfoMode::Channel
    }
    pub fn side_freq_hz(&self, index: usize) -> u32 {
        self.sides[index].cfg.freq_hz
    }
    pub fn side_tx_freq_hz(&self, index: usize) -> u32 {
        self.sides[index].cfg.tx_freq_hz
    }
    pub fn side_is_channel_mode(&self, index: usize) -> bool {
        self.sides[index].vfo_chan == ChVfoMode::Channel
    }
    pub fn side_channel_num(&self, index: usize) -> u16 {
        self.sides[index].channel_num
    }
    pub fn side_subaudio_rx(&self, index: usize) -> SubAudio {
        self.sides[index].cfg.subaudio_rx
    }
    pub fn side_subaudio_tx(&self, index: usize) -> SubAudio {
        self.sides[index].cfg.subaudio_tx
    }
    pub fn side_power(&self, index: usize) -> Power {
        self.sides[index].cfg.power
    }
    pub fn side_modulation(&self, index: usize) -> Modulation {
        self.sides[index].cfg.modulation
    }
    pub fn side_wide_band(&self, index: usize) -> bool {
        self.sides[index].cfg.wide_band
    }
    pub fn side_reversed(&self, index: usize) -> bool {
        self.sides[index].reversed
    }
    /// Repeater shift direction: 0 = off/simplex, 1 = `+`, 2 = `-`.
    pub fn side_freq_dir(&self, index: usize) -> u8 {
        self.sides[index].shift_dir_offset().0
    }
    pub fn side_offset_hz(&self, index: usize) -> u32 {
        self.sides[index].shift_dir_offset().1
    }
    /// Trimmed channel name, `""` in VFO mode or for an unnamed channel.
    pub fn side_name_str(&self, index: usize) -> &str {
        channel_name_str(&self.sides[index].name)
    }
    pub fn watching_index(&self) -> usize {
        self.watching
    }
    pub fn last_signal_side(&self) -> Option<usize> {
        self.last_signal_side
    }
    pub fn radio_mut(&mut self) -> &mut Radio {
        &mut self.radio
    }
    pub fn storage_mut(&mut self) -> &mut Storage {
        &mut self.storage
    }
    pub fn poll_squelch(&mut self, syst: &mut SYST, db: u8) {
        self.radio.poll_squelch(syst, db);
    }

    pub fn poll_scan(&mut self, syst: &mut SYST) {
        scan::poll(self, syst);
    }
    pub fn poll_search(&mut self, syst: &mut SYST) {
        search::poll(self, syst);
    }
    pub fn poll_scanqt(&mut self, syst: &mut SYST) {
        scanqt::poll(self, syst);
    }
    pub fn scan_direction_up(&self) -> bool {
        scan::direction_up(self)
    }
    pub fn search_band_label(&self) -> &'static str {
        search::band_label(self)
    }
    pub fn search_status(&self) -> SearchStatus {
        search::status(self)
    }
    pub fn search_candidate_freq_hz(&self) -> u32 {
        search::candidate_freq_hz(self)
    }
    pub fn search_tone(&self) -> Option<SubAudio> {
        search::tone(self)
    }
    pub fn scanqt_is_found(&self) -> bool {
        scanqt::is_found(self)
    }
    pub fn scanqt_is_listening(&self) -> bool {
        scanqt::is_listening(self)
    }
    pub fn scanqt_tone(&self) -> Option<SubAudio> {
        scanqt::tone(self)
    }
    pub fn poll_fm(&mut self, syst: &mut SYST) {
        fm::poll(self, syst);
    }
    pub fn fm_deci_mhz(&self) -> u16 {
        fm::deci_mhz(self)
    }
    pub fn fm_is_channel_mode(&self) -> bool {
        fm::is_channel_mode(self)
    }
    pub fn fm_channel_index(&self) -> u8 {
        fm::channel_index(self)
    }
    pub fn fm_is_seeking(&self) -> bool {
        fm::is_seeking(self)
    }
    pub fn fm_rssi(&self) -> u8 {
        fm::rssi(self)
    }
    pub fn fm_save_picker_selected(&self) -> Option<u8> {
        fm::save_picker_selected(self)
    }
    pub fn fm_channel_freq_at(&self, index: usize) -> Option<u16> {
        fm::channel_freq_at(self, index)
    }
    pub fn fm_input_len(&self) -> usize {
        fm::input_len(self)
    }
    pub fn fm_input_digit(&self, idx: usize) -> u8 {
        fm::input_digit(self, idx)
    }
    pub fn poll_spectrum(&mut self, syst: &mut SYST) {
        spectrum::poll(self, syst);
    }
    pub fn spectrum_window_start_hz(&self) -> u32 {
        spectrum::window_start_hz(self)
    }
    pub fn spectrum_scan_step_hz(&self) -> u32 {
        spectrum::scan_step_hz(self)
    }
    pub fn spectrum_pan_step_hz(&self) -> u32 {
        spectrum::pan_step_hz(self)
    }
    pub fn spectrum_bins(&self) -> u16 {
        spectrum::bins(self)
    }
    pub fn spectrum_rssi_bin(&self, pos: usize) -> u16 {
        spectrum::rssi_bin(self, pos)
    }
    pub fn spectrum_rssi_ceiling(&self) -> i16 {
        spectrum::rssi_ceiling(self)
    }
    pub fn spectrum_trigger_level(&self) -> u16 {
        spectrum::trigger_level(self)
    }
    pub fn spectrum_peak_bin(&self) -> Option<u16> {
        spectrum::peak_bin(self)
    }
    pub fn spectrum_listening(&self) -> bool {
        spectrum::listening(self)
    }
    pub fn spectrum_modulation(&self) -> Modulation {
        spectrum::modulation(self)
    }
    pub fn spectrum_wide_bandwidth(&self) -> bool {
        spectrum::wide_bandwidth(self)
    }
    pub fn spectrum_entering_freq(&self) -> bool {
        spectrum::entering_freq(self)
    }
    pub fn spectrum_input_len(&self) -> usize {
        spectrum::input_len(self)
    }
    pub fn spectrum_input_digit(&self, idx: usize) -> u8 {
        spectrum::input_digit(self, idx)
    }
    pub fn rssi_open(&self) -> bool {
        self.radio.rssi_open()
    }
    pub fn audio_open(&self) -> bool {
        self.radio.audio_is_open()
    }
    pub fn is_monitor(&self) -> bool {
        self.radio.is_monitor()
    }
    pub fn dual_standby_enabled(&self) -> bool {
        self.dual_standby
    }
    pub fn power_save_active(&self) -> bool {
        self.power_save
    }
    pub fn tx_elapsed_seconds(&self) -> u32 {
        self.tot_ticks as u32 / TICKS_PER_SECOND as u32
    }
    pub fn channel_display_mode(&self) -> ChannelDisplayMode {
        self.channel_display_mode
    }
    /// 0=Off, 1=Semi, 2=Full; see `flash_map::Settings::bk_in`.
    pub fn bk_in(&self) -> u8 {
        self.settings.bk_in
    }
    /// True while a CW/CWF key is physically held down, including
    /// BK-IN=Off's local-only sidetone
    pub fn cw_key_down(&self) -> bool {
        self.cw_key_down
    }
    pub fn set_channel_display_mode(&mut self, mode: ChannelDisplayMode) {
        self.channel_display_mode = mode;
    }

    pub fn freq_input_len(&self) -> usize {
        if matches!(self.sides[self.master].vfo_chan, ChVfoMode::Vfo) {
            self.input.len
        } else {
            0
        }
    }

    pub fn freq_input_digit(&self, idx: usize) -> u8 {
        self.input.digits[idx]
    }

    pub fn channel_input_len(&self) -> usize {
        if matches!(self.sides[self.master].vfo_chan, ChVfoMode::Channel) {
            self.input.len
        } else {
            0
        }
    }

    pub fn dtmf_dial_active(&self) -> bool {
        self.dtmf_dial.is_some()
    }

    pub fn dtmf_dial_len(&self) -> usize {
        self.dtmf_dial.as_ref().map_or(0, |d| d.len)
    }

    pub fn dtmf_dial_digit(&self, idx: usize) -> u8 {
        self.dtmf_dial.as_ref().map_or(0, |d| d.digits[idx])
    }

    pub fn dtmf_dial_capacity(&self) -> usize {
        DTMF_DIAL_MAX_DIGITS
    }
}
