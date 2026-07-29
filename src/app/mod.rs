mod menu;

use crate::drivers::fd6818::{Fd6818, Power, SubAudio};
use crate::drivers::keypad::{KeyEvent, KeyEventKind, KeyId, KeyManager};
use crate::drivers::norflash::NorFlash;
use crate::flash_map::{self, addr};
use crate::hal::wear_leveled::WearLeveledRegion;
use crate::radio::{ChannelConfig, Radio};
use cortex_m::peripheral::{SCB, SYST};
use menu::MenuItem;

const FIRMWARE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Channel spacing steps, in deci-Hz
const STEP_LIST_DECI_HZ: [u32; 9] = [250, 500, 625, 1000, 1250, 2000, 2500, 5000, 10000];
const DEFAULT_STEP_INDEX: u8 = 3; // 10kHz

const MAX_CHANNEL_NUM: u16 = 999;
const VFO_INPUT_DIGITS: usize = 8;
const CHANNEL_INPUT_DIGITS: usize = 3;

const DUAL_STANDBY_HOLD_TICKS: u16 = 10;

/// The scheduler ticks every 10ms.
const TICKS_PER_SECOND: u16 = 100;

/// VOX mic-level thresholds, indexed directly by the menu's `vox_level`
/// (1-9): level 1 is the most sensitive. The original's own default maps
/// to level 1 here (its `voxLevel` defaults to 0, indexing entry 1 of this
/// same table).
const VOX_THRESHOLD_TABLE: [u8; 11] = [127, 52, 62, 72, 84, 95, 106, 117, 125, 132, 140];

/// Once VOX has keyed TX, the release threshold drops by this much, so
/// normal speech dynamics don't flap the transmitter on and off.
const VOX_TX_HYSTERESIS: u8 = 8;

/// VOX hang time after the last above-threshold sample, in 10ms ticks
/// (1.0s). The original makes this a 0.5-2.0s setting (`voxDelay`,
/// default 1.0s); we fix it at the default rather than spend another menu
/// item on it.
const VOX_WORK_HOLD_TICKS: u8 = 100;

/// VOX hold-off (10ms ticks) after events whose audio tail would otherwise
/// false-trigger: a received signal (1.5s), a key beep (0.4s), a failed TX
/// entry (1.2s), or PTT release (1.0s).
const VOX_HOLD_AFTER_RX_TICKS: u8 = 150;
const VOX_HOLD_AFTER_KEY_TICKS: u8 = 40;
const VOX_HOLD_AFTER_TX_FAIL_TICKS: u8 = 120;
const VOX_HOLD_AFTER_PTT_TICKS: u8 = 100;

/// Repeater-access tone choices for `settings.rtone`, in units of 10Hz:
/// 1000/1450/1750/2100 Hz.
const RTONE_HZ_DIV_10: [u16; 4] = [100, 145, 175, 210];

/// Both VFO sides (A+B), combined into one wear-leveled record -- mirrors
/// `Flash_SaveVfoData`/`Flash_ReadVfoData`'s single 64-byte payload.
const VFO_REGION: WearLeveledRegion<64> = WearLeveledRegion::new(addr::VFO_INFO_ADDR, 16);

/// Global settings record, at the same address and header size the original
/// uses for `STR_RADIOINFORM` -- our payload is much smaller (see
/// `flash_map::Settings`), so this leaves most of the sector's slots unused.
const SETTINGS_REGION: WearLeveledRegion<16> = WearLeveledRegion::new(addr::RADIO_IMFOS_ADDR, 16);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standby,
    Menu,
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

fn subaudio_from_code(code: u16) -> SubAudio {
    match flash_map::SubaudioCode::decode(code) {
        flash_map::SubaudioCode::None => SubAudio::None,
        flash_map::SubaudioCode::Ctcss(tenths_hz) => SubAudio::Ctcss(tenths_hz),
        flash_map::SubaudioCode::DcsNormal(idx) => dcs_from_table_index(idx, false),
        flash_map::SubaudioCode::DcsInverted(idx) => dcs_from_table_index(idx - 105, true),
    }
}

/// `idx` is the 1-based `SubaudioCode` index (1..=105); out-of-range values
/// (malformed flash data) fall back to off rather than panicking.
fn dcs_from_table_index(idx: u16, inverted: bool) -> SubAudio {
    match menu::DCS_TABLE.get((idx as usize).wrapping_sub(1)) {
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

/// `cfg.freq_hz`/`cfg.tx_freq_hz` are derived from them by
/// `refresh_cfg_freqs()`, taking the reverse-swap flag into account.
struct Side {
    vfo_chan: ChVfoMode,
    channel_num: u16,
    freq_step: u8,
    rx_freq_hz: u32,
    tx_freq_hz: u32,
    freq_dir: u8,
    offset_hz: u32,
    reversed: bool,
    cfg: ChannelConfig,
}

impl Side {
    fn refresh_cfg_freqs(&mut self) {
        let (tx, rx) = match self.vfo_chan {
            ChVfoMode::Vfo => {
                let tx = match self.freq_dir {
                    1 => self.rx_freq_hz.saturating_add(self.offset_hz),
                    2 => self.rx_freq_hz.saturating_sub(self.offset_hz),
                    _ => self.rx_freq_hz,
                };
                (tx, self.rx_freq_hz)
            }
            ChVfoMode::Channel => (self.tx_freq_hz, self.rx_freq_hz),
        };
        if self.reversed {
            self.cfg.freq_hz = tx;
            self.cfg.tx_freq_hz = rx;
        } else {
            self.cfg.freq_hz = rx;
            self.cfg.tx_freq_hz = tx;
        }
    }

    fn load_vfo(&mut self, vfo: &flash_map::VfoMode) {
        self.vfo_chan = ChVfoMode::Vfo;
        self.rx_freq_hz = vfo.freq_deci_hz() * 10;
        self.freq_dir = vfo.freq_dir();
        self.offset_hz = vfo.offset_deci_hz() * 10;
        self.cfg.wide_band = !vfo.wide_narrow();
        self.cfg.power = power_from_raw(vfo.tx_power);
        self.cfg.subaudio_tx = subaudio_from_code(vfo.tx_dcs_cts_num);
        self.cfg.subaudio_rx = subaudio_from_code(vfo.rx_dcs_cts_num);
        self.refresh_cfg_freqs();
    }

    fn to_vfo_bytes(&self) -> [u8; addr::VFO_SIZE as usize] {
        let mut vfo = flash_map::VfoMode::from_bytes(&[0u8; addr::VFO_SIZE as usize]);
        vfo.set_freq_deci_hz(self.rx_freq_hz / 10);
        vfo.set_offset_deci_hz(self.offset_hz / 10);
        vfo.set_wide_narrow(!self.cfg.wide_band);
        vfo.tx_power = power_to_raw(self.cfg.power);
        vfo.rx_dcs_cts_num = subaudio_to_code(self.cfg.subaudio_rx);
        vfo.tx_dcs_cts_num = subaudio_to_code(self.cfg.subaudio_tx);
        vfo.dtmf_group = (self.freq_dir << 5) | (vfo.dtmf_group & 0x1f);
        vfo.to_bytes()
    }

    fn load_channel(&mut self, num: u16, ch: &flash_map::Channel) {
        self.vfo_chan = ChVfoMode::Channel;
        self.channel_num = num;
        self.rx_freq_hz = ch.rx_freq_deci_hz() * 10;
        self.tx_freq_hz = ch.tx_freq_deci_hz() * 10;
        self.cfg.wide_band = !ch.wide_narrow();
        self.cfg.power = power_from_raw(ch.tx_power);
        self.cfg.subaudio_tx = subaudio_from_code(ch.tx_dcs_cts_num);
        self.cfg.subaudio_rx = subaudio_from_code(ch.rx_dcs_cts_num);
        self.refresh_cfg_freqs();
    }

    fn step_deci_hz(&self) -> u32 {
        STEP_LIST_DECI_HZ[self.freq_step as usize]
    }
}

fn subaudio_to_code(sub: SubAudio) -> u16 {
    match sub {
        SubAudio::None => 0,
        SubAudio::Ctcss(tenths_hz) => tenths_hz,
        SubAudio::Dcs { code, inverted } => match menu::dcs_index(code) {
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

/// Combined CTCSS/DCS selector index space for the merged R-CTC/T-CTC menu
/// item: `-1` = off, `0..CTCSS_TABLE.len()` = CTCSS tones, then
/// `DCS_TABLE.len()` DCS-normal codes, then `DCS_TABLE.len()` DCS-inverted
/// codes.
const SUBAUDIO_MAX_INDEX: i32 = (menu::CTCSS_TABLE.len() + 2 * menu::DCS_TABLE.len() - 1) as i32;

fn subaudio_from_index(v: i32) -> SubAudio {
    if v < 0 {
        return SubAudio::None;
    }
    let v = v as usize;
    if v < menu::CTCSS_TABLE.len() {
        return SubAudio::Ctcss(menu::CTCSS_TABLE[v]);
    }
    let dcs_v = v - menu::CTCSS_TABLE.len();
    if dcs_v < menu::DCS_TABLE.len() {
        SubAudio::Dcs {
            code: menu::DCS_TABLE[dcs_v],
            inverted: false,
        }
    } else {
        let inv_v = (dcs_v - menu::DCS_TABLE.len()).min(menu::DCS_TABLE.len() - 1);
        SubAudio::Dcs {
            code: menu::DCS_TABLE[inv_v],
            inverted: true,
        }
    }
}

fn subaudio_index(sub: SubAudio) -> i32 {
    match sub {
        SubAudio::None => -1,
        SubAudio::Ctcss(hz) => match menu::ctcss_index(Some(hz)) {
            Some(i) => i as i32,
            None => -1,
        },
        SubAudio::Dcs { code, inverted } => match menu::dcs_index(code) {
            Some(i) => {
                let base =
                    menu::CTCSS_TABLE.len() + if inverted { menu::DCS_TABLE.len() } else { 0 };
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

struct MenuState {
    index: usize,
    /// Adjusting the current item's value (vs. still browsing between
    /// items). Also doubles as "viewing"/"confirming" for `Info`/`Reset`,
    /// which don't have a plain adjustable value.
    editing: bool,
    /// `menu_current_value()` at the moment editing started, restored if
    /// the user backs out with Exit instead of confirming with Menu.
    snapshot: i32,
    /// Which of `Info`'s two sub-screens is shown.
    info_page: u8,
}

impl MenuState {
    const fn new() -> Self {
        MenuState {
            index: 0,
            editing: false,
            snapshot: 0,
            info_page: 0,
        }
    }
}

pub struct App<'a> {
    pub radio: Radio<'a>,
    keys: KeyManager<'a>,
    norflash: NorFlash<'a>,
    mode: Mode,
    sides: [Side; 2],
    settings: flash_map::Settings,
    menu: MenuState,

    master: usize,
    watching: usize,
    last_signal_side: Option<usize>,

    input: InputBuf,
    key_lock: bool,
    transmitting: bool,
    dual_standby: bool,
    dual_hold_ticks: u16,
    tot_ticks: u16,

    /// Idle countdown (100ms ticks) toward the automatic key lock, reloaded
    /// from `settings.key_auto_lock * 50` on any user activity.
    key_idle_ticks: u16,
    /// VOX hold-off before detection resumes, 10ms ticks.
    vox_det_dly: u8,
    /// VOX TX hang time remaining, 10ms ticks.
    vox_work_dly: u8,
    /// True while the transmitter is keyed by VOX rather than the PTT key,
    /// so `poll_vox` knows it's the one responsible for unkeying.
    vox_active: bool,
    /// Access tone currently sounding (side key held during TX).
    rtone_sounding: bool,
}

impl<'a> App<'a> {
    pub fn new(
        mut radio: Radio<'a>,
        keys: KeyManager<'a>,
        mut norflash: NorFlash<'a>,
        default_cfg: ChannelConfig,
        syst: &mut SYST,
    ) -> Self {
        // --- RF chip init and factory calibration ---
        radio.init(syst);

        let mut cal_buf = [0u8; 16];
        norflash.read_bytes(addr::RF_MODULATION_ADDR, &mut cal_buf);
        let xtal_adjust = cal_buf[6];
        radio.fd6818_mut().set_xtal_adjust(xtal_adjust);
        radio
            .fd6818_mut()
            .set_audio_calibration(cal_buf[0], cal_buf[1], cal_buf[2], cal_buf[3], cal_buf[4]);
        radio.fd6818_mut().apply_af_calibration(
            syst,
            cal_buf[11],
            cal_buf[12],
            cal_buf[13],
            cal_buf[14],
        );

        if let Some(pa_addr) = Fd6818::pa_target_addr(default_cfg.freq_hz, default_cfg.power) {
            let mut pa_byte = [0u8; 1];
            norflash.read_bytes(pa_addr, &mut pa_byte);
            radio.fd6818_mut().set_pa_calibration(pa_byte[0]);
        }

        let vfo_payload = VFO_REGION.load(&mut norflash);
        let settings = SETTINGS_REGION
            .load(&mut norflash)
            .map(|b| flash_map::Settings::from_bytes(&b))
            .unwrap_or(flash_map::Settings::DEFAULT);

        let mut sides = [
            Side {
                vfo_chan: ChVfoMode::Vfo,
                channel_num: 1,
                freq_step: DEFAULT_STEP_INDEX,
                rx_freq_hz: default_cfg.freq_hz,
                tx_freq_hz: default_cfg.tx_freq_hz,
                freq_dir: 0,
                offset_hz: 0,
                reversed: false,
                cfg: default_cfg,
            },
            Side {
                vfo_chan: ChVfoMode::Vfo,
                channel_num: 1,
                freq_step: DEFAULT_STEP_INDEX,
                rx_freq_hz: default_cfg.freq_hz,
                tx_freq_hz: default_cfg.tx_freq_hz,
                freq_dir: 0,
                offset_hz: 0,
                reversed: false,
                cfg: default_cfg,
            },
        ];

        if let Some(buf) = vfo_payload {
            for (half, side) in sides.iter_mut().enumerate() {
                let bytes: [u8; addr::VFO_SIZE as usize] = buf
                    [half * addr::VFO_SIZE as usize..(half + 1) * addr::VFO_SIZE as usize]
                    .try_into()
                    .unwrap();
                let vfo = flash_map::VfoMode::from_bytes(&bytes);
                if vfo.freq_deci_hz() != 0 {
                    side.load_vfo(&vfo);
                }
            }
        }

        radio.set_frequency(sides[0].cfg.freq_hz);
        radio.set_tx_frequency(sides[0].cfg.tx_freq_hz);
        radio.set_power(sides[0].cfg.power);
        radio.set_subaudio_tx(sides[0].cfg.subaudio_tx);
        radio.set_subaudio_rx(sides[0].cfg.subaudio_rx);
        radio.set_sql_level(syst, settings.sql_level);
        radio.set_tail_elimination(settings.tail_elimination);
        radio.set_beeps_enabled(settings.beeps_switch);
        radio.set_roger_beep(settings.roger_beep);
        radio.set_scramble_level(syst, settings.scramble_level);

        App {
            radio,
            keys,
            norflash,
            mode: Mode::Standby,
            sides,
            settings,
            menu: MenuState::new(),
            master: 0,
            watching: 0,
            last_signal_side: None,
            input: InputBuf::new(),
            key_lock: false,
            transmitting: false,
            dual_standby: settings.dual_standby,
            dual_hold_ticks: DUAL_STANDBY_HOLD_TICKS,
            tot_ticks: 0,
            key_idle_ticks: 0,
            vox_det_dly: 0,
            vox_work_dly: 0,
            vox_active: false,
            rtone_sounding: false,
        }
    }

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

    fn save_vfo(&mut self) {
        let mut buf = [0u8; 64];
        for (half, side) in self.sides.iter().enumerate() {
            buf[half * 32..(half + 1) * 32].copy_from_slice(&side.to_vfo_bytes());
        }
        VFO_REGION.save(&mut self.norflash, &buf);
    }

    fn save_settings(&mut self) {
        SETTINGS_REGION.save(&mut self.norflash, &self.settings.to_bytes());
    }

    fn load_channel_num(&mut self, num: u16) {
        let addr = addr::CHAN_ADDR + num as u32 * addr::CHAN_SIZE;
        let mut buf = [0u8; addr::CHAN_SIZE as usize];
        self.norflash.read_bytes(addr, &mut buf);
        let ch = flash_map::Channel::from_bytes(&buf);
        self.sides[self.master].load_channel(num, &ch);
    }

    fn apply_watching_to_radio(&mut self, syst: &mut SYST) {
        let side = &self.sides[self.watching];
        self.radio.set_frequency(side.cfg.freq_hz);
        self.radio.set_tx_frequency(side.cfg.tx_freq_hz);
        self.radio.set_power(side.cfg.power);
        self.radio.set_subaudio_tx(side.cfg.subaudio_tx);
        self.radio.set_subaudio_rx(side.cfg.subaudio_rx);
        if !self.transmitting {
            self.radio.enter_rx(syst);
        }
    }

    fn sync_watching_to_master(&mut self, syst: &mut SYST) {
        self.watching = self.master;
        self.apply_watching_to_radio(syst);
    }

    fn step(&mut self, syst: &mut SYST, up: bool) {
        let side = &mut self.sides[self.master];
        match side.vfo_chan {
            ChVfoMode::Vfo => {
                let step_hz = side.step_deci_hz() * 10;
                side.rx_freq_hz = if up {
                    side.rx_freq_hz.saturating_add(step_hz)
                } else {
                    side.rx_freq_hz.saturating_sub(step_hz)
                };
                side.refresh_cfg_freqs();
                self.save_vfo();
            }
            ChVfoMode::Channel => {
                let next = if up {
                    if side.channel_num >= MAX_CHANNEL_NUM {
                        1
                    } else {
                        side.channel_num + 1
                    }
                } else if side.channel_num <= 1 {
                    MAX_CHANNEL_NUM
                } else {
                    side.channel_num - 1
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
                let side = &mut self.sides[self.master];
                side.rx_freq_hz = value * 10;
                side.refresh_cfg_freqs();
                self.save_vfo();
                self.sync_watching_to_master(syst);
            }
        }
        self.input.clear();
    }

    fn toggle_vfo_channel(&mut self, syst: &mut SYST) {
        let side = &mut self.sides[self.master];
        side.vfo_chan = match side.vfo_chan {
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
        let side = &mut self.sides[self.master];
        side.reversed = !side.reversed;
        side.refresh_cfg_freqs();
        self.sync_watching_to_master(syst);
    }

    fn toggle_power(&mut self, syst: &mut SYST) {
        let side = &mut self.sides[self.master];
        side.cfg.power = match side.cfg.power {
            Power::Low => Power::High,
            _ => Power::Low,
        };
        self.sync_watching_to_master(syst);
    }

    /// Bench-test hook: keys PTT on the master side and sends DTMF "123"
    /// over RF, for checking the DTMF encoder against a real decoder.
    /// Long-press Digit9 in standby.
    fn test_send_dtmf(&mut self, syst: &mut SYST) {
        self.set_ptt(syst, true);
        self.radio.send_dtmf_digits(syst, &[1, 2, 3]);
        self.set_ptt(syst, false);
    }

    /// Reloads the auto-lock idle countdown. Called on any user activity
    /// (key event, PTT edge, leaving the menu); the countdown itself only
    /// runs while idle in standby, see `poll_auto_lock`.
    fn reset_key_idle(&mut self) {
        self.key_idle_ticks = self.settings.key_auto_lock as u16 * 50;
    }

    /// Auto key lock countdown. Call every 100ms. The timer only runs down
    /// while the radio is genuinely idle -- sitting in standby, not
    /// transmitting, no signal open -- so it can't lock the keys out from
    /// under an ongoing contact. `rx_active` is the squelch/audio-open
    /// state from the main loop.
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

    /// VOX detector. Call every 10ms with the mic level (12-bit ADC
    /// reading shifted down to 8 bits) and the current audio-open state.
    ///
    /// Above `VOX_THRESHOLD_TABLE[vox_level]`, the hang timer reloads
    /// and (in RX) the transmitter keys; once the timer runs out, VOX
    /// unkeys again -- but only a TX it keyed itself (`vox_active`), never
    /// a manual PTT. After events with an audio tail (received signal, key
    /// beep, PTT release) detection holds off for a while so the tail
    /// doesn't retrigger it.
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
        // Menu (or any non-standby mode) and a just-received signal both
        // hold detection off; `rx_active` is only meaningful in RX, since
        // the main loop stops refreshing it while transmitting.
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
                    // TXINH/BCL refused the key-up; back off briefly
                    // instead of retrying on every tick.
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

    fn dispatch_key(&mut self, syst: &mut SYST, ev: KeyEvent) {
        // everything except the side keys and long-press unlock is swallowed
        // while locked.
        if self.key_lock
            && !matches!(ev.key, KeyId::Side1 | KeyId::Side2)
            && !matches!((ev.key, ev.kind), (KeyId::Asterisk, KeyEventKind::Long))
        {
            return;
        }

        self.reset_key_idle();
        // The beep's audio tail would otherwise false-trigger VOX on the
        // next detection window.
        self.vox_det_dly = VOX_HOLD_AFTER_KEY_TICKS;

        // While transmitting, the keypad does nothing but the access-tone
        // burst on Side2 -- the ordinary standby handlers must not retune
        // or reorder anything underneath an in-progress TX.
        if self.transmitting && self.mode == Mode::Standby {
            self.dispatch_key_tx(syst, ev);
            return;
        }

        // Not `Repeat` -- held Up/Down would otherwise machine-gun the beep.
        if matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Long) {
            self.radio.play_beep(syst);
        }

        match self.mode {
            Mode::Standby => self.dispatch_key_standby(syst, ev),
            Mode::Menu => self.dispatch_key_menu(syst, ev),
            // TODO: Fm/Scan/Search/... aren't implemented yet.
            _ => {}
        }
    }

    /// Key handling while PTT is held: holding Side2 sounds the
    /// repeater-access tone selected by `settings.rtone` until release.
    /// Everything else is ignored on purpose.
    fn dispatch_key_tx(&mut self, syst: &mut SYST, ev: KeyEvent) {
        if ev.key != KeyId::Side2 {
            return;
        }
        match ev.kind {
            KeyEventKind::Press => {
                let idx = self.settings.rtone.min(3) as usize;
                self.radio.rtone_on(syst, RTONE_HZ_DIV_10[idx]);
                self.rtone_sounding = true;
            }
            KeyEventKind::Release => {
                if self.rtone_sounding {
                    self.radio.rtone_off(syst);
                    self.rtone_sounding = false;
                }
            }
            _ => {}
        }
    }

    fn dispatch_key_standby(&mut self, syst: &mut SYST, ev: KeyEvent) {
        // Any standby keypress holds off dual standby's side-swapping
        // for a full window again, so it doesn't yank the watched side
        // out from under whatever the user is doing.
        self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;

        match ev.kind {
            KeyEventKind::Single => {
                if let Some(d) = digit_value(ev.key) {
                    let max_len = match self.sides[self.master].vfo_chan {
                        ChVfoMode::Channel => CHANNEL_INPUT_DIGITS,
                        ChVfoMode::Vfo => VFO_INPUT_DIGITS,
                    };
                    self.input.push(d);
                    if self.input.len >= max_len {
                        self.commit_input(syst);
                    }
                    return;
                }
                match ev.key {
                    KeyId::Up => self.step(syst, true),
                    KeyId::Down => self.step(syst, false),
                    KeyId::Exit => self.input.clear(),
                    KeyId::Vm => self.toggle_vfo_channel(syst),
                    KeyId::Ab => self.switch_side(syst),
                    KeyId::Asterisk => self.toggle_reverse(syst),
                    KeyId::Menu => self.menu_enter(),
                    _ => {}
                }
            }
            KeyEventKind::Repeat => match ev.key {
                KeyId::Up => self.step(syst, true),
                KeyId::Down => self.step(syst, false),
                _ => {}
            },
            KeyEventKind::Long => match ev.key {
                KeyId::Asterisk => self.key_lock = !self.key_lock,
                KeyId::Digit8 => self.toggle_power(syst),
                KeyId::Digit9 => self.test_send_dtmf(syst),
                // TODO: Search/lock-display/weather/dual-standby/monitor remaps
                // all target modes that aren't built yet.
                _ => {}
            },
            _ => {}
        }
    }

    fn dispatch_key_menu(&mut self, syst: &mut SYST, ev: KeyEvent) {
        let item = menu::MENU_ORDER[self.menu.index];

        match ev.kind {
            KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
                KeyId::Up | KeyId::Down => {
                    let up = ev.key == KeyId::Up;
                    if !self.menu.editing {
                        let len = menu::MENU_ORDER.len();
                        self.menu.index = if up {
                            (self.menu.index + 1) % len
                        } else {
                            (self.menu.index + len - 1) % len
                        };
                    } else if item == MenuItem::Info {
                        self.menu.info_page ^= 1;
                    } else if item != MenuItem::Reset {
                        self.menu_adjust(syst, item, up);
                    }
                }
                KeyId::Menu if ev.kind == KeyEventKind::Single => {
                    if !self.menu.editing {
                        if item.is_placeholder() {
                            return;
                        }
                        self.menu.snapshot = self.menu_current_value(item);
                        self.menu.editing = true;
                    } else if item == MenuItem::Reset {
                        self.factory_reset();
                    } else {
                        self.menu.editing = false;
                    }
                }
                KeyId::Exit if ev.kind == KeyEventKind::Single => {
                    if self.menu.editing {
                        if item != MenuItem::Info && item != MenuItem::Reset {
                            self.menu_apply_value(syst, item, self.menu.snapshot);
                        }
                        self.menu.editing = false;
                    } else {
                        self.menu_exit();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn menu_enter(&mut self) {
        self.mode = Mode::Menu;
        self.menu.editing = false;
        self.input.clear();
    }

    fn menu_exit(&mut self) {
        self.save_settings();
        self.save_vfo();
        self.mode = Mode::Standby;
        // Back to standby with a fresh idle window so the keys don't lock
        // the moment the menu closes.
        self.reset_key_idle();
    }

    fn factory_reset(&mut self) -> ! {
        self.norflash.erase_sector(addr::VFO_INFO_ADDR);
        self.norflash.erase_sector(addr::RADIO_IMFOS_ADDR);
        SCB::sys_reset();
    }

    fn menu_current_value(&self, item: MenuItem) -> i32 {
        let side = &self.sides[self.master];
        match item {
            MenuItem::Sql => self.settings.sql_level as i32,
            MenuItem::Step => side.freq_step as i32,
            MenuItem::Tot => self.settings.tot_level as i32,
            MenuItem::Tdr => self.settings.dual_standby as i32,
            MenuItem::BusyLock => self.settings.busy_lock as i32,
            MenuItem::TxForbid => self.settings.tx_forbid as i32,
            MenuItem::Beep => self.settings.beeps_switch as i32,
            MenuItem::Roge => self.settings.roger_beep as i32,
            MenuItem::Wn => !side.cfg.wide_band as i32,
            MenuItem::TxPr => matches!(side.cfg.power, Power::Low) as i32,
            MenuItem::RxCts => subaudio_index(side.cfg.subaudio_rx),
            MenuItem::TxCts => subaudio_index(side.cfg.subaudio_tx),
            MenuItem::Scrm => self.settings.scramble_level as i32,
            MenuItem::Sftd => side.freq_dir as i32,
            MenuItem::Offse => side.offset_hz as i32,
            MenuItem::AutoLk => self.settings.key_auto_lock as i32,
            MenuItem::Vox => self.settings.vox_switch as i32,
            MenuItem::VoxLv => self.settings.vox_level as i32,
            MenuItem::Rtone => self.settings.rtone as i32,
            MenuItem::Contrast => self.settings.contrast as i32,
            MenuItem::Info => self.menu.info_page as i32,
            _ => 0,
        }
    }

    fn menu_adjust(&mut self, syst: &mut SYST, item: MenuItem, up: bool) {
        let cur = self.menu_current_value(item);
        let side_step_hz = self.sides[self.master].step_deci_hz() * 10;

        let new_val = match item {
            MenuItem::Sql => clamp_step(cur, up, 0, 9),
            MenuItem::Step => clamp_step(cur, up, 0, STEP_LIST_DECI_HZ.len() as i32 - 1),
            MenuItem::Tot => clamp_step(cur, up, 0, 12),
            MenuItem::Sftd => clamp_step(cur, up, 0, 2),
            MenuItem::Tdr
            | MenuItem::BusyLock
            | MenuItem::TxForbid
            | MenuItem::Beep
            | MenuItem::Roge
            | MenuItem::Wn
            | MenuItem::TxPr => 1 - cur,
            MenuItem::RxCts | MenuItem::TxCts => wrap_step(cur, up, -1, SUBAUDIO_MAX_INDEX),
            MenuItem::Scrm => clamp_step(cur, up, 0, 3),
            MenuItem::AutoLk => clamp_step(cur, up, 0, 3),
            MenuItem::Vox => 1 - cur,
            MenuItem::VoxLv => clamp_step(cur, up, 1, 9),
            MenuItem::Rtone => clamp_step(cur, up, 0, 3),
            MenuItem::Contrast => clamp_step(cur, up, 0, 4),
            MenuItem::Offse => {
                if up {
                    cur.saturating_add(side_step_hz as i32)
                } else {
                    (cur - side_step_hz as i32).max(0)
                }
            }
            _ => cur,
        };
        self.menu_apply_value(syst, item, new_val);
    }

    fn menu_apply_value(&mut self, syst: &mut SYST, item: MenuItem, v: i32) {
        match item {
            MenuItem::Sql => {
                self.settings.sql_level = v as u8;
                self.radio.set_sql_level(syst, v as u8);
            }
            MenuItem::Step => self.sides[self.master].freq_step = v as u8,
            MenuItem::Tot => self.settings.tot_level = v as u8,
            MenuItem::Tdr => {
                self.settings.dual_standby = v != 0;
                self.set_dual_standby(v != 0);
            }
            MenuItem::BusyLock => self.settings.busy_lock = v != 0,
            MenuItem::TxForbid => self.settings.tx_forbid = v != 0,
            MenuItem::Beep => {
                self.settings.beeps_switch = v != 0;
                self.radio.set_beeps_enabled(v != 0);
            }
            MenuItem::Roge => {
                self.settings.roger_beep = v != 0;
                self.radio.set_roger_beep(v != 0);
            }
            MenuItem::Wn => {
                self.sides[self.master].cfg.wide_band = v == 0;
                self.commit_side_change(syst);
            }
            MenuItem::TxPr => {
                self.sides[self.master].cfg.power = if v != 0 { Power::Low } else { Power::High };
                self.commit_side_change(syst);
            }
            MenuItem::RxCts => {
                self.sides[self.master].cfg.subaudio_rx = subaudio_from_index(v);
                self.commit_side_change(syst);
            }
            MenuItem::TxCts => {
                self.sides[self.master].cfg.subaudio_tx = subaudio_from_index(v);
                self.commit_side_change(syst);
            }
            MenuItem::Scrm => {
                self.settings.scramble_level = v as u8;
                self.radio.set_scramble_level(syst, v as u8);
            }
            MenuItem::Sftd => {
                self.sides[self.master].freq_dir = v as u8;
                self.commit_side_change(syst);
            }
            MenuItem::Offse => {
                self.sides[self.master].offset_hz = v as u32;
                self.commit_side_change(syst);
            }
            MenuItem::AutoLk => {
                self.settings.key_auto_lock = v as u8;
                self.reset_key_idle();
            }
            MenuItem::Vox => {
                self.settings.vox_switch = v != 0;
                if v == 0 && self.vox_active {
                    // Don't leave a VOX-keyed transmitter stranded with no
                    // owner to unkey it.
                    self.vox_active = false;
                    self.vox_work_dly = 0;
                    self.set_ptt(syst, false);
                }
            }
            MenuItem::VoxLv => self.settings.vox_level = v as u8,
            MenuItem::Rtone => self.settings.rtone = v as u8,
            MenuItem::Contrast => self.settings.contrast = v as u8,
            _ => {}
        }
    }

    /// VFO-mode edits persist (`save_vfo`); channel-mode edits are
    /// RAM-only for now -- channel storage/rename/delete is a CPS PC-tool
    /// job, so we never write the channel record back.
    fn commit_side_change(&mut self, syst: &mut SYST) {
        self.sides[self.master].refresh_cfg_freqs();
        if matches!(self.sides[self.master].vfo_chan, ChVfoMode::Vfo) {
            self.save_vfo();
        }
        self.sync_watching_to_master(syst);
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn menu_item_label(&self) -> &'static str {
        menu::MENU_ORDER[self.menu.index].label()
    }

    pub fn menu_editing(&self) -> bool {
        self.menu.editing
    }

    pub fn menu_value_text<W: core::fmt::Write>(&self, w: &mut W) {
        let item = menu::MENU_ORDER[self.menu.index];
        if item.is_placeholder() {
            let _ = write!(w, "----");
            return;
        }
        match item {
            MenuItem::Info => {
                let _ = if self.menu.info_page == 0 {
                    write!(w, "FW {}", FIRMWARE_VERSION)
                } else {
                    write!(w, "UV-K6")
                };
            }
            MenuItem::Reset => {
                if self.menu.editing {
                    let _ = write!(w, "Sure? MENU");
                }
            }
            MenuItem::Step => {
                let deci_hz = STEP_LIST_DECI_HZ[self.menu_current_value(item) as usize];
                let _ = write!(w, "{}.{:02}k", deci_hz / 100, deci_hz % 100);
            }
            MenuItem::Tdr
            | MenuItem::BusyLock
            | MenuItem::TxForbid
            | MenuItem::Beep
            | MenuItem::Roge => {
                let _ = write!(
                    w,
                    "{}",
                    if self.menu_current_value(item) != 0 {
                        "ON"
                    } else {
                        "OFF"
                    }
                );
            }
            MenuItem::AutoLk => {
                let _ = write!(
                    w,
                    "{}",
                    match self.menu_current_value(item) {
                        1 => "5S",
                        2 => "10S",
                        3 => "15S",
                        _ => "OFF",
                    }
                );
            }
            MenuItem::Vox => {
                let _ = write!(
                    w,
                    "{}",
                    if self.menu_current_value(item) != 0 {
                        "ON"
                    } else {
                        "OFF"
                    }
                );
            }
            MenuItem::Rtone => {
                let idx = self.menu_current_value(item).clamp(0, 3) as usize;
                let _ = write!(w, "{}Hz", RTONE_HZ_DIV_10[idx] as u32 * 10);
            }
            MenuItem::Wn => {
                let _ = write!(
                    w,
                    "{}",
                    if self.menu_current_value(item) != 0 {
                        "NARROW"
                    } else {
                        "WIDE"
                    }
                );
            }
            MenuItem::TxPr => {
                let _ = write!(
                    w,
                    "{}",
                    if self.menu_current_value(item) != 0 {
                        "LOW"
                    } else {
                        "HIGH"
                    }
                );
            }
            MenuItem::RxCts | MenuItem::TxCts => {
                let side = &self.sides[self.master];
                let sub = if item == MenuItem::RxCts {
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
            MenuItem::Scrm => {
                let v = self.menu_current_value(item);
                if v == 0 {
                    let _ = write!(w, "OFF");
                } else {
                    let _ = write!(w, "{}", v);
                }
            }
            MenuItem::Offse => {
                let hz = self.menu_current_value(item) as u32;
                let _ = write!(w, "{}.{:03}k", hz / 1000, hz % 1000);
            }
            _ => {
                let _ = write!(w, "{}", self.menu_current_value(item));
            }
        }
    }

    pub fn poll_keys(&mut self, syst: &mut SYST) {
        self.keys.poll(syst);
        while let Some(ev) = self.keys.pop_event() {
            self.dispatch_key(syst, ev);
        }
    }

    /// Re-reads the calibrated APC target byte for the frequency/power about
    /// to be used and pushes it into the driver. `pa_target` only takes
    /// effect on the next `pa_enable()` call, so this has to run before
    /// every `enter_tx()` -- previously it was only ever loaded once, at
    /// boot, for the startup default frequency/power, so every later TX
    /// (any channel, either power level) silently kept using that one
    /// stale value.
    fn reload_pa_calibration(&mut self, tx_freq_hz: u32, power: Power) {
        if let Some(addr) = Fd6818::pa_target_addr(tx_freq_hz, power) {
            let mut buf = [0u8; 1];
            self.norflash.read_bytes(addr, &mut buf);
            self.radio.fd6818_mut().set_pa_calibration(buf[0]);
        }
    }

    pub fn set_ptt(&mut self, syst: &mut SYST, pressed: bool) {
        if pressed && !self.transmitting {
            if self.settings.tx_forbid {
                return;
            }
            if self.settings.busy_lock && self.radio.rssi_open() {
                return;
            }

            // TX on master side
            self.watching = self.master;
            let side = &self.sides[self.master];
            let tx_freq_hz = side.cfg.tx_freq_hz;
            let power = side.cfg.power;
            self.radio.set_frequency(side.cfg.freq_hz);
            self.radio.set_tx_frequency(tx_freq_hz);
            self.radio.set_power(power);
            self.radio.set_subaudio_tx(side.cfg.subaudio_tx);
            self.radio.set_subaudio_rx(side.cfg.subaudio_rx);

            self.reload_pa_calibration(tx_freq_hz, power);

            self.transmitting = true;
            self.tot_ticks = 0;
            self.radio.enter_tx(syst);
        } else if pressed {
            // A physical PTT press landing while VOX holds the transmitter
            // means the user takes over; VOX must not unkey under their
            // thumb.
            self.vox_active = false;
            self.vox_work_dly = 0;
        } else if !pressed && self.transmitting {
            self.transmitting = false;
            if self.rtone_sounding {
                // TX is about to be torn down anyway; just stop tracking
                // the tone so a later key release doesn't "restore" TX
                // state into what is now RX.
                self.rtone_sounding = false;
            }
            self.radio.end_tx(syst);
            self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
            // Hold VOX off for a moment: the TX tail (roger beep, tail
            // elimination) would otherwise immediately retrigger it.
            self.vox_det_dly = VOX_HOLD_AFTER_PTT_TICKS;
            self.vox_work_dly = 0;
            self.vox_active = false;
        }
    }

    /// Forces TX off once `settings.tot_level` (steps of 15s) has elapsed;
    /// `tot_level == 0` disables the timeout. Call unconditionally every
    /// scheduler tick, same as `poll_dual_standby`.
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

    pub fn is_transmitting(&self) -> bool {
        self.transmitting
    }

    pub fn is_key_locked(&self) -> bool {
        self.key_lock
    }

    /// LCD contrast (electronic volume) level, 0-4. main.rs applies it to
    /// the display controller whenever it changes.
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

    pub fn last_signal_side(&self) -> Option<usize> {
        self.last_signal_side
    }
}
