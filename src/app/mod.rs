mod menu;

use crate::drivers::fd6818::{Power, SubAudio};
use crate::drivers::keypad::{KeyEvent, KeyEventKind, KeyId, KeyManager};
use crate::drivers::norflash::NorFlash;
use crate::flash_map::{self, addr};
use crate::hal::wear_leveled::WearLeveledRegion;
use crate::radio::{ChannelConfig, Radio};
use cortex_m::peripheral::{SYST, SCB};
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

/// Both VFO sides (A+B), combined into one wear-leveled record -- mirrors
/// `Flash_SaveVfoData`/`Flash_ReadVfoData`'s single 64-byte payload.
const VFO_REGION: WearLeveledRegion<64> = WearLeveledRegion::new(addr::VFO_INFO_ADDR, 16);

/// Global settings record, at the same address and header size the original
/// uses for `STR_RADIOINFORM` -- our payload is much smaller (see
/// `flash_map::Settings`), so this leaves most of the sector's slots unused.
const SETTINGS_REGION: WearLeveledRegion<11> = WearLeveledRegion::new(addr::RADIO_IMFOS_ADDR, 16);

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
        flash_map::SubaudioCode::Ctcss(tenths_hz) => SubAudio::Ctcss(tenths_hz),
        // TODO: DCS isn't implemented in the fd6818 driver yet.
        _ => SubAudio::None,
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
    }
}

fn subaudio_ctcss_hz(sub: SubAudio) -> Option<u16> {
    match sub {
        SubAudio::Ctcss(tenths_hz) => Some(tenths_hz),
        SubAudio::None => None,
    }
}

/// `v < 0` means off.
fn ctcss_from_index(v: i32) -> SubAudio {
    if v < 0 {
        SubAudio::None
    } else {
        SubAudio::Ctcss(menu::CTCSS_TABLE[v as usize])
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
}

impl<'a> App<'a> {
    pub fn new(
        mut radio: Radio<'a>,
        keys: KeyManager<'a>,
        mut norflash: NorFlash<'a>,
        default_cfg: ChannelConfig,
        syst: &mut SYST,
    ) -> Self {
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

    fn dispatch_key(&mut self, syst: &mut SYST, ev: KeyEvent) {
        // everything except the side keys and long-press unlock is swallowed
        // while locked.
        if self.key_lock
            && !matches!(ev.key, KeyId::Side1 | KeyId::Side2)
            && !matches!((ev.key, ev.kind), (KeyId::Asterisk, KeyEventKind::Long))
        {
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
            MenuItem::RxCts => match menu::ctcss_index(subaudio_ctcss_hz(side.cfg.subaudio_rx)) {
                Some(i) => i as i32,
                None => -1,
            },
            MenuItem::TxCts => match menu::ctcss_index(subaudio_ctcss_hz(side.cfg.subaudio_tx)) {
                Some(i) => i as i32,
                None => -1,
            },
            MenuItem::Sftd => side.freq_dir as i32,
            MenuItem::Offse => side.offset_hz as i32,
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
            MenuItem::RxCts | MenuItem::TxCts => {
                wrap_step(cur, up, -1, menu::CTCSS_TABLE.len() as i32 - 1)
            }
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
                self.sides[self.master].cfg.subaudio_rx = ctcss_from_index(v);
                self.commit_side_change(syst);
            }
            MenuItem::TxCts => {
                self.sides[self.master].cfg.subaudio_tx = ctcss_from_index(v);
                self.commit_side_change(syst);
            }
            MenuItem::Sftd => {
                self.sides[self.master].freq_dir = v as u8;
                self.commit_side_change(syst);
            }
            MenuItem::Offse => {
                self.sides[self.master].offset_hz = v as u32;
                self.commit_side_change(syst);
            }
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
            MenuItem::Tdr | MenuItem::BusyLock | MenuItem::TxForbid | MenuItem::Beep | MenuItem::Roge => {
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
                let v = self.menu_current_value(item);
                if v < 0 {
                    let _ = write!(w, "OFF");
                } else {
                    let hz = menu::CTCSS_TABLE[v as usize];
                    let _ = write!(w, "{}.{}Hz", hz / 10, hz % 10);
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
            self.radio.set_frequency(side.cfg.freq_hz);
            self.radio.set_tx_frequency(side.cfg.tx_freq_hz);
            self.radio.set_power(side.cfg.power);
            self.radio.set_subaudio_tx(side.cfg.subaudio_tx);
            self.radio.set_subaudio_rx(side.cfg.subaudio_rx);

            self.transmitting = true;
            self.tot_ticks = 0;
            self.radio.enter_tx(syst);
        } else if !pressed && self.transmitting {
            self.transmitting = false;
            self.radio.end_tx(syst);
            self.dual_hold_ticks = DUAL_STANDBY_HOLD_TICKS;
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
