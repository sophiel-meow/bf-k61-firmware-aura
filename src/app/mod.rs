mod keys;
mod launcher;
mod settings;
mod settings_ops;
mod side;

use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId, Keypad};
use crate::device::radio::{ChannelConfig, Modulation, Power, Radio, SubAudio};
use crate::device::storage::Storage;
use crate::flash_map::{self, addr};
use cortex_m::peripheral::SYST;

const FIRMWARE_VERSION: &str = env!("CARGO_PKG_VERSION");

const STEP_LIST_DECI_HZ: [u32; 9] = [250, 500, 625, 1000, 1250, 2000, 2500, 5000, 10000];
const DEFAULT_STEP_INDEX: u8 = 3;

const MAX_CHANNEL_NUM: u16 = 999;

const VFO_INPUT_DIGITS: usize = 6;
const CHANNEL_INPUT_DIGITS: usize = 3;

const DUAL_STANDBY_HOLD_TICKS: u16 = 10;
const TICKS_PER_SECOND: u16 = 100;

const ANI_DISPLAY_HOLD_TICKS: u16 = 500;

const VOX_THRESHOLD_TABLE: [u8; 11] = [127, 52, 62, 72, 84, 95, 106, 117, 125, 132, 140];
const VOX_TX_HYSTERESIS: u8 = 8;
const VOX_WORK_HOLD_TICKS: u8 = 100;
const VOX_HOLD_AFTER_RX_TICKS: u8 = 150;
const VOX_HOLD_AFTER_KEY_TICKS: u8 = 40;
const VOX_HOLD_AFTER_TX_FAIL_TICKS: u8 = 120;
const VOX_HOLD_AFTER_PTT_TICKS: u8 = 100;

const RTONE_HZ_DIV_10: [u16; 4] = [100, 145, 175, 210];

// Mode
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standby,
    AppMenu,
    Settings,
    Fm,
    Moni,
    Scan,
    Search,
    ScanQt,
    Weather,
    StopWatch,
    Dtmf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChVfoMode {
    Vfo,
    Channel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelDisplayMode {
    Frequency,
    Name,
    NameFreq,
}

fn channel_name_str(raw: &[u8; 12]) -> &str {
    let end = raw
        .iter()
        .position(|&b| b == 0x00 || b == 0xFF)
        .unwrap_or(raw.len());
    core::str::from_utf8(&raw[..end]).unwrap_or("")
}

// conversion helpers
fn subaudio_from_code(code: u16) -> SubAudio {
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

fn power_from_raw(tx_power: u8) -> Power {
    if tx_power & 0x01 != 0 {
        Power::Low
    } else {
        Power::High
    }
}

fn power_to_raw(power: Power) -> u8 {
    match power {
        Power::Low => 1,
        _ => 0,
    }
}

fn digit_value(key: KeyId) -> Option<u8> {
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

fn subaudio_to_code(sub: SubAudio) -> u16 {
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

const SUBAUDIO_MAX_INDEX: i32 =
    (settings::CTCSS_TABLE.len() + 2 * settings::DCS_TABLE.len() - 1) as i32;

fn subaudio_from_index(v: i32) -> SubAudio {
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

fn subaudio_index(sub: SubAudio) -> i32 {
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

fn clamp_step(cur: i32, up: bool, lo: i32, hi: i32) -> i32 {
    if up {
        (cur + 1).min(hi)
    } else {
        (cur - 1).max(lo)
    }
}

fn wrap_step(cur: i32, up: bool, lo: i32, hi: i32) -> i32 {
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

// InputBuf

struct InputBuf {
    digits: [u8; VFO_INPUT_DIGITS],
    len: usize,
}

impl InputBuf {
    const fn new() -> Self {
        InputBuf {
            digits: [0; VFO_INPUT_DIGITS],
            len: 0,
        }
    }
    fn clear(&mut self) {
        self.len = 0;
    }
    fn push(&mut self, digit: u8) {
        if self.len < self.digits.len() {
            self.digits[self.len] = digit;
            self.len += 1;
        }
    }
    fn value(&self) -> u32 {
        self.digits[..self.len]
            .iter()
            .fold(0u32, |acc, &d| acc * 10 + d as u32)
    }
}

// SettingsUi
struct SettingsUi {
    index: usize,
    editing: bool,
    snapshot: i32,
    info_page: u8,
}

impl SettingsUi {
    const fn new() -> Self {
        SettingsUi {
            index: 0,
            editing: false,
            snapshot: 0,
            info_page: 0,
        }
    }
}

pub struct App {
    pub radio: Radio,
    keypad: Keypad,
    storage: Storage,
    mode: Mode,
    sides: [side::Side; 2],
    settings: flash_map::Settings,
    settings_ui: SettingsUi,
    launcher_index: usize,

    master: usize,
    watching: usize,
    last_signal_side: Option<usize>,

    input: InputBuf,
    key_lock: bool,
    transmitting: bool,
    tx_prohibited: bool,
    dual_standby: bool,
    dual_hold_ticks: u16,
    tot_ticks: u16,

    key_idle_ticks: u16,
    vox_det_dly: u8,
    vox_work_dly: u8,
    vox_active: bool,
    rtone_sounding: bool,

    ani_caller: Option<[u8; 3]>,
    ani_hold_ticks: u16,

    battery_cal: [u8; 7],
    battery_bars: u8,

    channel_display_mode: ChannelDisplayMode,

    // TODO: power save
    power_save: bool,
}

impl App {
    pub fn new(
        mut radio: Radio,
        keypad: Keypad,
        mut storage: Storage,
        default_cfg: ChannelConfig,
        syst: &mut SYST,
    ) -> Self {
        let vfo_payload = storage.load_vfo_raw();
        let settings = storage
            .load_settings()
            .unwrap_or(flash_map::Settings::DEFAULT);
        let battery_cal = storage.read_battery_calibration();

        let mut sides = [
            side::Side {
                vfo_chan: ChVfoMode::Vfo,
                channel_num: 1,
                freq_step: DEFAULT_STEP_INDEX,
                rx_freq_hz: default_cfg.freq_hz,
                tx_freq_hz: default_cfg.tx_freq_hz,
                freq_dir: 0,
                offset_hz: 0,
                reversed: false,
                cfg: default_cfg,
                name: [0; 12],
            },
            side::Side {
                vfo_chan: ChVfoMode::Vfo,
                channel_num: 1,
                freq_step: DEFAULT_STEP_INDEX,
                rx_freq_hz: default_cfg.freq_hz,
                tx_freq_hz: default_cfg.tx_freq_hz,
                freq_dir: 0,
                offset_hz: 0,
                reversed: false,
                cfg: default_cfg,
                name: [0; 12],
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

        radio.set_frequency(sides[0].cfg.freq_hz);
        radio.set_tx_frequency(sides[0].cfg.tx_freq_hz);
        radio.set_power(sides[0].cfg.power);
        radio.set_subaudio_tx(sides[0].cfg.subaudio_tx);
        radio.set_subaudio_rx(sides[0].cfg.subaudio_rx);
        radio.set_modulation(sides[0].cfg.modulation);
        radio.set_sql_level(syst, settings.sql_level);
        radio.set_tail_elimination(settings.tail_elimination);
        radio.set_beeps_enabled(settings.beeps_switch);
        radio.set_roger_beep(settings.roger_beep);
        radio.set_scramble_level(syst, settings.scramble_level);

        App {
            radio,
            keypad,
            storage,
            mode: Mode::Standby,
            sides,
            settings,
            settings_ui: SettingsUi::new(),
            launcher_index: 0,
            master: 0,
            watching: 0,
            last_signal_side: None,
            input: InputBuf::new(),
            key_lock: false,
            transmitting: false,
            tx_prohibited: false,
            dual_standby: settings.dual_standby,
            dual_hold_ticks: DUAL_STANDBY_HOLD_TICKS,
            tot_ticks: 0,
            key_idle_ticks: 0,
            vox_det_dly: 0,
            vox_work_dly: 0,
            vox_active: false,
            rtone_sounding: false,
            ani_caller: None,
            ani_hold_ticks: 0,
            battery_cal,
            battery_bars: 4,
            channel_display_mode: ChannelDisplayMode::Frequency,
            power_save: false,
        }
    }

    // dual standby
    pub fn set_dual_standby(&mut self, enabled: bool) {
        self.dual_standby = enabled;
        self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
    }

    pub fn poll_dual_standby(&mut self, syst: &mut SYST, signal_present: bool) {
        if signal_present {
            self.last_signal_side = Some(self.watching);
        }
        if !self.dual_standby || self.mode != Mode::Standby || self.transmitting {
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

    fn load_channel_num(&mut self, num: u16) {
        let ch = self.storage.read_channel(num);
        self.sides[self.master].load_channel(num, &ch);
    }

    // radio sync
    fn apply_watching_to_radio(&mut self, syst: &mut SYST) {
        let s = &self.sides[self.watching];
        self.radio.set_frequency(s.cfg.freq_hz);
        self.radio.set_tx_frequency(s.cfg.tx_freq_hz);
        self.radio.set_power(s.cfg.power);
        self.radio.set_subaudio_tx(s.cfg.subaudio_tx);
        self.radio.set_subaudio_rx(s.cfg.subaudio_rx);
        self.radio.set_modulation(s.cfg.modulation);
        if !self.transmitting {
            self.radio.enter_rx(syst);
        }
    }

    fn sync_watching_to_master(&mut self, syst: &mut SYST) {
        self.watching = self.master;
        self.apply_watching_to_radio(syst);
    }

    // frequency / channel stepping
    fn step(&mut self, syst: &mut SYST, up: bool) {
        let s = &mut self.sides[self.master];
        match s.vfo_chan {
            ChVfoMode::Vfo => {
                let step_hz = s.step_deci_hz() * 10;
                s.rx_freq_hz = if up {
                    s.rx_freq_hz.saturating_add(step_hz)
                } else {
                    s.rx_freq_hz.saturating_sub(step_hz)
                };
                s.refresh_cfg_freqs();
                self.save_vfo();
            }
            ChVfoMode::Channel => {
                let next = if up {
                    if s.channel_num >= MAX_CHANNEL_NUM {
                        1
                    } else {
                        s.channel_num + 1
                    }
                } else if s.channel_num <= 1 {
                    MAX_CHANNEL_NUM
                } else {
                    s.channel_num - 1
                };
                self.load_channel_num(next);
            }
        }
        self.sync_watching_to_master(syst);
    }

    fn commit_input(&mut self, syst: &mut SYST) {
        let value = self.input.value();
        match self.sides[self.master].vfo_chan {
            ChVfoMode::Channel => {
                if value >= 1 && value <= MAX_CHANNEL_NUM as u32 {
                    self.load_channel_num(value as u16);
                    self.sync_watching_to_master(syst);
                }
            }
            ChVfoMode::Vfo => {
                let s = &mut self.sides[self.master];
                // `value` is 6 digits of kHz (`"xxx.xxx"` with the dot
                // implied); `* 1000` converts to Hz.
                s.rx_freq_hz = value * 1000;
                s.refresh_cfg_freqs();
                self.save_vfo();
                self.sync_watching_to_master(syst);
            }
        }
        self.input.clear();
    }

    fn toggle_vfo_channel(&mut self, syst: &mut SYST) {
        let s = &mut self.sides[self.master];
        s.vfo_chan = match s.vfo_chan {
            ChVfoMode::Vfo => ChVfoMode::Channel,
            ChVfoMode::Channel => ChVfoMode::Vfo,
        };
        self.input.clear();
        self.sync_watching_to_master(syst);
    }

    fn switch_side(&mut self, syst: &mut SYST) {
        self.master = 1 - self.master;
        self.input.clear();
        self.sync_watching_to_master(syst);
    }

    fn toggle_reverse(&mut self, syst: &mut SYST) {
        let s = &mut self.sides[self.master];
        s.reversed = !s.reversed;
        s.refresh_cfg_freqs();
        self.sync_watching_to_master(syst);
    }

    fn toggle_power(&mut self, syst: &mut SYST) {
        let s = &mut self.sides[self.master];
        s.cfg.power = match s.cfg.power {
            Power::Low => Power::High,
            _ => Power::Low,
        };
        self.sync_watching_to_master(syst);
    }

    fn toggle_modulation(&mut self, syst: &mut SYST) {
        let s = &mut self.sides[self.master];
        s.cfg.modulation = match s.cfg.modulation {
            Modulation::Fm => Modulation::Am,
            Modulation::Am => Modulation::Fm,
        };
        self.sync_watching_to_master(syst);
    }

    fn commit_side_change(&mut self, syst: &mut SYST) {
        self.sides[self.master].refresh_cfg_freqs();
        if matches!(self.sides[self.master].vfo_chan, ChVfoMode::Vfo) {
            self.save_vfo();
        }
        self.sync_watching_to_master(syst);
    }

    // test hook
    fn test_send_dtmf(&mut self, syst: &mut SYST) {
        self.set_ptt(syst, true);
        self.radio.send_dtmf_digits(syst, &[1, 2, 3]);
        self.set_ptt(syst, false);
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
            self.vox_work_dly = VOX_WORK_HOLD_TICKS;
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
        let raw = (self.radio.read_battery_raw() >> 4) as u8;
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

    // key dispatch
    pub fn poll_keys(&mut self, syst: &mut SYST) {
        self.keypad.poll(syst);
        while let Some(ev) = self.keypad.pop_event() {
            keys::dispatch(self, syst, ev);
        }
    }

    pub fn launcher_value_text<W: core::fmt::Write>(&self, w: &mut W) {
        keys::launcher_value_text(self, w);
    }

    // settings UI
    pub fn settings_item_label(&self) -> &'static str {
        settings_ops::item_label(self)
    }

    pub fn settings_editing(&self) -> bool {
        settings_ops::editing(self)
    }

    pub fn settings_value_text<W: core::fmt::Write>(&self, w: &mut W) {
        settings_ops::value_text(self, w)
    }

    // PTT
    fn reload_pa_calibration(&mut self, tx_freq_hz: u32, power: Power) {
        self.radio
            .apply_pa_calibration(&mut self.storage, tx_freq_hz, power);
    }

    pub fn set_ptt(&mut self, syst: &mut SYST, pressed: bool) {
        if pressed && !self.transmitting {
            if self.settings.tx_forbid {
                return;
            }
            if self.settings.busy_lock && self.radio.rssi_open() {
                return;
            }

            self.watching = self.master;
            let s = &self.sides[self.master];
            let tx_freq_hz = s.cfg.tx_freq_hz;
            let power = s.cfg.power;
            self.radio.set_frequency(s.cfg.freq_hz);
            self.radio.set_tx_frequency(tx_freq_hz);
            self.radio.set_power(power);
            self.radio.set_subaudio_tx(s.cfg.subaudio_tx);
            self.radio.set_subaudio_rx(s.cfg.subaudio_rx);
            self.reload_pa_calibration(tx_freq_hz, power);
            if self.radio.enter_tx(syst) {
                self.transmitting = true;
                self.tot_ticks = 0;
                self.tx_prohibited = false;
            } else {
                self.tx_prohibited = true;
            }
        } else if pressed {
            self.vox_active = false;
            self.vox_work_dly = 0;
        } else {
            // PTT released.
            self.tx_prohibited = false;
            if self.transmitting {
                self.transmitting = false;
                if self.rtone_sounding {
                    self.rtone_sounding = false;
                }
                self.radio.end_tx(syst);
                self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
                self.vox_det_dly = VOX_HOLD_AFTER_PTT_TICKS;
                self.vox_work_dly = 0;
                self.vox_active = false;
            }
        }
    }

    // TOT
    pub fn poll_tot(&mut self, syst: &mut SYST) {
        if !self.transmitting || self.settings.tot_level == 0 {
            return;
        }
        self.tot_ticks += 1;
        let limit = self.settings.tot_seconds() * TICKS_PER_SECOND;
        if self.tot_ticks >= limit {
            self.set_ptt(syst, false);
        }
    }

    // getters
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn is_transmitting(&self) -> bool {
        self.transmitting
    }
    pub fn tx_prohibited(&self) -> bool {
        self.tx_prohibited
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
    /// Repeater shift direction: 0 = off/simplex, 1 = `+`, 2 = `-`.
    pub fn side_freq_dir(&self, index: usize) -> u8 {
        self.sides[index].freq_dir
    }
    pub fn side_offset_hz(&self, index: usize) -> u32 {
        self.sides[index].offset_hz
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
    pub fn poll_squelch(&mut self, syst: &mut SYST, db: u8) {
        self.radio.poll_squelch(syst, db);
    }
    pub fn rssi_open(&self) -> bool {
        self.radio.rssi_open()
    }
    pub fn audio_open(&self) -> bool {
        self.radio.audio_is_open()
    }
    pub fn dual_standby_enabled(&self) -> bool {
        self.dual_standby
    }
    // TODO: power saving
    pub fn power_save_active(&self) -> bool {
        self.power_save
    }
    pub fn tx_elapsed_seconds(&self) -> u32 {
        self.tot_ticks as u32 / TICKS_PER_SECOND as u32
    }
    pub fn channel_display_mode(&self) -> ChannelDisplayMode {
        self.channel_display_mode
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
}
