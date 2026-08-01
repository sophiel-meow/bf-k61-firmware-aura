use crate::board;
use crate::drivers::fd6818::{AfOutState, Fd6818};
use crate::hal::{adc, debounce, delay};
use cortex_m::peripheral::SYST;
use kd32f328_pac::{gpioa, gpiof};

pub use crate::drivers::fd6818::{Modulation, Power, RawTone, SubAudio};

/// How long `end_tx()` holds the tail-elimination tone before actually
/// cutting the carrier.
const SEND_TAIL_HOLD_MS: u32 = 300;

/// 1000Hz/60ms
const BEEP_TONES: [(u16, u32); 1] = [(100, 60)];
/// 1000Hz/80ms, 850Hz/80ms
const ROGER_TONES: [(u16, u32); 2] = [(100, 80), (85, 80)];

const TONE_SETTLE_MS: u32 = 2;

/// OpenGD77-style Boot-tune note table: standard 12-tone equal temperament,
/// A2=110Hz, values in `hz_div_10`
/// index 0 is `tone_index` 1 (A2), so a boot- tune pair looks up
/// `BOOT_TUNE_HZ_DIV_10[tone_index - 1]`.
pub const BOOT_TUNE_HZ_DIV_10: [u16; 45] = [
    11, 12, 12, 13, 14, 15, 16, 16, 17, 19, 20, 21, 22, 23, 25, 26, 28, 29, 31, 33, 35, 37, 39, 42,
    44, 47, 49, 52, 55, 59, 62, 66, 70, 74, 78, 83, 88, 93, 99, 105, 111, 117, 124, 132, 140,
];

/// One period unit in an OpenGD77-style boot-tune `(tone_index, duration)`
/// pair.
const BOOT_TUNE_PERIOD_MS: u32 = 30;

/// `txOffTone`: what plays right after PTT release, before the tail
/// elimination tone (if any) and the return to RX.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RogerTone {
    Off,
    /// roger beep
    Roger,
    /// MDC1200 burst
    Mdc1200,
}

impl RogerTone {
    pub fn from_u8(v: u8) -> RogerTone {
        match v {
            1 => RogerTone::Roger,
            2 => RogerTone::Mdc1200,
            _ => RogerTone::Off,
        }
    }
}

/// ADC channel for VOX mic level (PA0 = ADC1 ch0).
pub const VOX_ADC_CHANNEL: u8 = 0;
/// ADC channel for battery voltage (PA1 = ADC1 ch1).
pub const BATT_ADC_CHANNEL: u8 = 1;

// TX frequency allow-list
/// A closed frequency window in Hz: `[low_hz, high_hz]`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FreqRange {
    pub low_hz: u32,
    pub high_hz: u32,
}

impl FreqRange {
    pub const fn contains(&self, freq_hz: u32) -> bool {
        freq_hz >= self.low_hz && freq_hz <= self.high_hz
    }
}

const MAX_FREQ_RANGES: usize = 8;

#[derive(Clone, Copy)]
pub struct FreqRanges {
    ranges: [FreqRange; MAX_FREQ_RANGES],
    count: u8,
}

impl FreqRanges {
    pub const fn empty() -> Self {
        FreqRanges {
            ranges: [FreqRange {
                low_hz: 0,
                high_hz: 0,
            }; MAX_FREQ_RANGES],
            count: 0,
        }
    }

    pub fn from_slice(windows: &[FreqRange]) -> Self {
        let mut out = Self::empty();
        for &w in windows.iter().take(MAX_FREQ_RANGES) {
            out.ranges[out.count as usize] = w;
            out.count += 1;
        }
        out
    }

    /// The FD6818's own synthesizer capability
    /// value from datasheet, may be different amoung chips
    pub fn hardware() -> Self {
        Self::from_slice(&[
            FreqRange {
                low_hz: 16_000_000,
                high_hz: 560_000_000,
            },
            FreqRange {
                low_hz: 740_000_000,
                high_hz: 1_120_000_000,
            },
        ])
    }

    pub fn allows(&self, freq_hz: u32) -> bool {
        self.ranges[..self.count as usize]
            .iter()
            .any(|r| r.contains(freq_hz))
    }

    /// Overall `(lowest low_hz, highest high_hz)` across all defined
    /// windows, for wrapping a stepped scan at the outer edges, not a
    /// membership test (the gap between disjoint windows, if any, isn't
    /// considered).
    pub fn bounds(&self) -> (u32, u32) {
        let active = &self.ranges[..self.count as usize];
        let lo = active.iter().map(|r| r.low_hz).min().unwrap_or(0);
        let hi = active.iter().map(|r| r.high_hz).max().unwrap_or(u32::MAX);
        (lo, hi)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BandLock {
    /// EU & CN amateur: 144-146MHz + 430-440MHz
    Ce,
    /// US amateur: 144-148MHz + 420-450MHz.
    Fcc,
    /// UK amateur: 144-148MHz + 430-440MHz.
    Gb,
    /// 137-174MHz + 400-430MHz.
    Mhz430,
    /// 137-174MHz + 400-438MHz.
    Mhz438,
    /// PMR446: 446.00625-446.19375MHz.
    Pmr,
    /// FRS/GMRS (462.550-462.725MHz + 467.550-467.725MHz) plus the 5 fixed
    /// MURS channels.
    GmrsFrsMurs,
    /// Canadian amateur: 144-148MHz + 430-450MHz.
    Ca,
    /// TX disabled on every frequency.
    All,
    /// No restriction beyond the hardware's own synthesizer range
    None,
}

impl BandLock {
    pub fn from_u8(v: u8) -> BandLock {
        match v {
            1 => BandLock::Fcc,
            2 => BandLock::Gb,
            3 => BandLock::Mhz430,
            4 => BandLock::Mhz438,
            5 => BandLock::Pmr,
            6 => BandLock::GmrsFrsMurs,
            7 => BandLock::Ca,
            8 => BandLock::All,
            9 => BandLock::None,
            _ => BandLock::Ce,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BandLock::Ce => "CE HAM",
            BandLock::Fcc => "FCC HAM",
            BandLock::Gb => "GB HAM",
            BandLock::Mhz430 => "400-430",
            BandLock::Mhz438 => "400-438",
            BandLock::Pmr => "PMR446",
            BandLock::GmrsFrsMurs => "GMRS/FRS",
            BandLock::Ca => "CA HAM",
            BandLock::All => "ALL LOCK",
            BandLock::None => "UNLOCK",
        }
    }

    pub fn tx_ranges(self) -> FreqRanges {
        const fn r(low_hz: u32, high_hz: u32) -> FreqRange {
            FreqRange { low_hz, high_hz }
        }
        match self {
            BandLock::Fcc => {
                FreqRanges::from_slice(&[r(144_000_000, 148_000_000), r(420_000_000, 450_000_000)])
            }
            BandLock::Ce => {
                FreqRanges::from_slice(&[r(144_000_000, 146_000_000), r(430_000_000, 440_000_000)])
            }
            BandLock::Gb => {
                FreqRanges::from_slice(&[r(144_000_000, 148_000_000), r(430_000_000, 440_000_000)])
            }
            BandLock::Mhz430 => {
                FreqRanges::from_slice(&[r(137_000_000, 174_000_000), r(400_000_000, 430_000_000)])
            }
            BandLock::Mhz438 => {
                FreqRanges::from_slice(&[r(137_000_000, 174_000_000), r(400_000_000, 438_000_000)])
            }
            BandLock::Pmr => FreqRanges::from_slice(&[r(446_006_250, 446_193_750)]),
            BandLock::GmrsFrsMurs => FreqRanges::from_slice(&[
                r(462_550_000, 462_725_000),
                r(467_550_000, 467_725_000),
                r(151_820_000, 151_820_000),
                r(151_880_000, 151_880_000),
                r(151_940_000, 151_940_000),
                r(154_570_000, 154_570_000),
                r(154_600_000, 154_600_000),
            ]),
            BandLock::Ca => {
                FreqRanges::from_slice(&[r(144_000_000, 148_000_000), r(430_000_000, 450_000_000)])
            }
            BandLock::All => FreqRanges::empty(),
            BandLock::None => FreqRanges::hardware(),
        }
    }
}

// types
#[derive(Clone, Copy, PartialEq, Eq)]
enum Band {
    Vhf,
    Uhf,
}

#[derive(Clone, Copy)]
pub struct ChannelConfig {
    pub freq_hz: u32,
    pub tx_freq_hz: u32,
    pub wide_band: bool,
    pub power: Power,
    pub subaudio_tx: SubAudio,
    pub subaudio_rx: SubAudio,
    pub modulation: Modulation,
}

#[derive(Clone, Copy)]
pub struct AniConfig {
    pub machine_id: [u8; 3],
    separator: u8,
    group_code: Option<u8>,
}

/// Separator symbol table, indexed by the flash `separator` field (0-5):
/// A, B, C, D, `*`, `#`.
const SEPARATOR_SYMBOLS: [u8; 6] = [10, 11, 12, 13, 14, 15];
/// Group-call symbol table, indexed by the flash `group_call` field (0-6):
/// off, A, B, C, D, `*`, `#`.
const GROUP_SYMBOLS: [Option<u8>; 7] = [
    None,
    Some(10),
    Some(11),
    Some(12),
    Some(13),
    Some(14),
    Some(15),
];

impl AniConfig {
    pub fn from_raw(machine_id: [u8; 3], separator_idx: u8, group_idx: u8) -> Self {
        let separator_idx = if separator_idx > 5 { 4 } else { separator_idx };
        let group_idx = if group_idx > 6 { 5 } else { group_idx };
        AniConfig {
            machine_id,
            separator: SEPARATOR_SYMBOLS[separator_idx as usize],
            group_code: GROUP_SYMBOLS[group_idx as usize],
        }
    }
}

const ANI_FRAME_LEN: usize = 7;
const DTMF_RX_TIMEOUT_TICKS: u8 = 40;
const DTMF_RX_MAX_DIGITS: usize = 16;

pub struct Radio {
    fd6818: Fd6818<'static>,
    gpioa: &'static gpioa::RegisterBlock,
    gpiob: &'static gpiof::RegisterBlock,
    adc: adc::Adc<'static>,
    ptt_debouncer: debounce::Debouncer,
    cfg: ChannelConfig,

    sql_level: u8,
    tail_elimination: bool,
    rptrl: u8,
    beeps_enabled: bool,
    roger_tone: RogerTone,
    scramble_level: u8,

    rit_offset_hz: i32,

    audio_open: bool,
    sq_debounce: u8,

    rssi_open: bool,
    rssi_debounce: u8,

    tx_state: bool,

    ani: AniConfig,
    dtmf_rx_buf: [u8; DTMF_RX_MAX_DIGITS],
    dtmf_rx_len: u8,
    dtmf_rx_timeout: u8,

    tx_allowed: FreqRanges,
    monitor: bool,
}

impl Radio {
    pub fn new(
        fd6818: Fd6818<'static>,
        gpioa: &'static gpioa::RegisterBlock,
        gpiob: &'static gpiof::RegisterBlock,
        adc_regs: &'static kd32f328_pac::adc::RegisterBlock,
        cfg: ChannelConfig,
        ani: AniConfig,
    ) -> Self {
        Radio {
            fd6818,
            gpioa,
            gpiob,
            adc: adc::Adc::new(adc_regs),
            ptt_debouncer: debounce::Debouncer::new(board::read_ptt(gpioa)),
            cfg,
            sql_level: 3,
            tail_elimination: true,
            rptrl: 0,
            beeps_enabled: true,
            roger_tone: RogerTone::Off,
            scramble_level: 0,
            rit_offset_hz: 0,
            audio_open: false,
            sq_debounce: 0,
            rssi_open: false,
            rssi_debounce: 0,
            tx_state: false,
            ani,
            dtmf_rx_buf: [0; DTMF_RX_MAX_DIGITS],
            dtmf_rx_len: 0,
            dtmf_rx_timeout: 0,
            tx_allowed: FreqRanges::hardware(),
            monitor: false,
        }
    }

    /// Cut all RF power immediately, for power-off.
    pub fn rf_off(&mut self, syst: &mut SYST) {
        self.fd6818.rf_off(syst);
    }

    /// Park the two-way receiver while the FM broadcast chip is active.
    pub fn park_for_fm(&mut self, syst: &mut SYST) {
        self.fd6818.idle(syst);
        self.fd6818.pa_off(syst);
        board::set_speaker_switch(self.gpiob, false);
    }

    pub fn set_speaker(&mut self, on: bool) {
        board::set_speaker_switch(self.gpiob, on);
    }

    /// Load PA calibration for the given TX frequency+power from flash,
    /// pushing it into the FD6818 driver. Returns `true` if a calibration
    /// address was found.
    pub fn apply_pa_calibration(
        &mut self,
        storage: &mut crate::device::storage::Storage,
        freq_hz: u32,
        power: Power,
    ) -> bool {
        if let Some(target) = storage.read_pa_calibration(freq_hz, power) {
            self.fd6818.set_pa_calibration(target);
            true
        } else {
            false
        }
    }

    pub fn set_tx_allowed(&mut self, ranges: FreqRanges) {
        self.tx_allowed = ranges;
    }

    // ADC

    /// 12-bit ADC reading for the VOX mic envelope (ch0), shifted down to
    /// 8 bits
    pub fn read_mic_level(&mut self) -> u8 {
        (self.adc.read_channel(VOX_ADC_CHANNEL) >> 4) as u8
    }

    // #[allow(dead_code)]
    // pub fn read_mic_raw(&mut self) -> u16 {
    //     self.adc.read_channel(VOX_ADC_CHANNEL)
    // }

    pub fn read_battery_raw(&mut self) -> u16 {
        self.adc.read_channel(BATT_ADC_CHANNEL)
    }

    // PTT

    /// Poll the hardware PTT line. Returns `Some(true)` on press,
    /// `Some(false)` on release, `None` when steady.
    pub fn poll_ptt(&mut self) -> Option<bool> {
        self.ptt_debouncer.sample(board::read_ptt(self.gpioa))
    }

    /// Raw PTT state, for power-on latch timing.
    pub fn ptt_asserted(&self) -> bool {
        !board::read_ptt(self.gpioa)
    }

    // band
    fn band(&self) -> Band {
        if self.cfg.freq_hz >= 300_000_000 {
            Band::Uhf
        } else {
            Band::Vhf
        }
    }

    // config setters
    pub fn set_frequency(&mut self, freq_hz: u32) {
        self.cfg.freq_hz = freq_hz;
    }

    pub fn set_tx_frequency(&mut self, freq_hz: u32) {
        self.cfg.tx_freq_hz = freq_hz;
    }

    pub fn set_power(&mut self, power: Power) {
        self.cfg.power = power;
    }

    pub fn set_subaudio_tx(&mut self, sub: SubAudio) {
        self.cfg.subaudio_tx = sub;
    }

    pub fn set_subaudio_rx(&mut self, sub: SubAudio) {
        self.cfg.subaudio_rx = sub;
    }

    pub fn set_modulation(&mut self, modulation: Modulation) {
        self.cfg.modulation = modulation;
    }

    pub fn set_sql_level(&mut self, syst: &mut SYST, level: u8) {
        self.sql_level = level;
        self.fd6818.set_squelch_level(syst, self.cfg.freq_hz, level);
    }

    pub fn set_tail_elimination(&mut self, enabled: bool) {
        self.tail_elimination = enabled;
    }

    pub fn set_rptrl(&mut self, steps: u8) {
        self.rptrl = steps;
    }

    pub fn set_beeps_enabled(&mut self, enabled: bool) {
        self.beeps_enabled = enabled;
    }

    pub fn set_roger_tone(&mut self, tone: RogerTone) {
        self.roger_tone = tone;
    }

    pub fn set_scramble_level(&mut self, syst: &mut SYST, level: u8) {
        self.scramble_level = level;
        self.fd6818.set_scramble(syst, level);
    }

    pub fn set_rit_offset(&mut self, hz: i32) {
        self.rit_offset_hz = hz;
    }

    // DTMF
    pub fn send_dtmf_digits(&mut self, syst: &mut SYST, digits: &[u8]) {
        const LEAD_IN_MS: u32 = 100;
        const TONE_MS: u32 = 80;
        const GAP_MS: u32 = 80;
        self.fd6818.enter_dtmf_mode(syst, true);
        delay::ms(syst, LEAD_IN_MS);
        for &digit in digits {
            self.fd6818.set_dtmf_digit(syst, Some(digit));
            delay::ms(syst, TONE_MS);
            self.fd6818.set_dtmf_digit(syst, None);
            delay::ms(syst, GAP_MS);
        }
        self.fd6818.exit_dtmf_mode(syst);
        self.fd6818.set_scramble(syst, self.scramble_level);
        self.fd6818.enter_dtmf_mode(syst, false);
    }

    pub fn send_ani(&mut self, syst: &mut SYST, target: [u8; 3]) {
        let digits = [
            target[0],
            target[1],
            target[2],
            self.ani.separator,
            self.ani.machine_id[0],
            self.ani.machine_id[1],
            self.ani.machine_id[2],
        ];
        self.send_dtmf_digits(syst, &digits);
    }

    pub fn poll_dtmf(&mut self, syst: &mut SYST) -> Option<[u8; 3]> {
        if !self.audio_open {
            self.dtmf_rx_len = 0;
            self.dtmf_rx_timeout = 0;
            return None;
        }

        if self.fd6818.dtmf_digit_ready(syst) {
            let digit = self.fd6818.read_dtmf_digit(syst);
            let len = self.dtmf_rx_len as usize;
            if len < DTMF_RX_MAX_DIGITS {
                self.dtmf_rx_buf[len] = digit;
                self.dtmf_rx_len += 1;
            }
            self.dtmf_rx_timeout = DTMF_RX_TIMEOUT_TICKS;
            if self.dtmf_rx_len as usize >= DTMF_RX_MAX_DIGITS {
                return self.finish_dtmf_frame();
            }
            return None;
        }

        if self.dtmf_rx_timeout > 0 {
            self.dtmf_rx_timeout -= 1;
            if self.dtmf_rx_timeout == 0 && self.dtmf_rx_len > 0 {
                return self.finish_dtmf_frame();
            }
        }
        None
    }

    fn finish_dtmf_frame(&mut self) -> Option<[u8; 3]> {
        let len = self.dtmf_rx_len as usize;
        self.dtmf_rx_len = 0;

        if len != ANI_FRAME_LEN || self.dtmf_rx_buf[3] != self.ani.separator {
            return None;
        }
        let call_code = [
            self.dtmf_rx_buf[0],
            self.dtmf_rx_buf[1],
            self.dtmf_rx_buf[2],
        ];
        let is_individual = call_code == self.ani.machine_id;
        let is_group = match self.ani.group_code {
            Some(g) => call_code == [g, g, g],
            None => false,
        };
        if !is_individual && !is_group {
            return None;
        }
        let mut caller = [0u8; 3];
        caller.copy_from_slice(&self.dtmf_rx_buf[4..7]);
        Some(caller)
    }

    // tones
    pub fn play_beep(&mut self, syst: &mut SYST) {
        if !self.beeps_enabled || self.audio_open {
            return;
        }
        self.play_tone_sequence(syst, &BEEP_TONES, false, true);
    }

    fn play_roger_tone(&mut self, syst: &mut SYST) {
        self.play_tone_sequence(syst, &ROGER_TONES, true, false);
    }

    /// Plays a boot-tune melody: `(tone_index, duration)` pairs,
    /// `tone_index` 0 = rest (silence for `duration` periods), 1..=45
    /// indexes `BOOT_TUNE_HZ_DIV_10`. A `(0, 0)` pair ends the tune early,
    /// and so does any `tone_index` outside `0..=45` -- there's no valid
    /// reason for one to appear in real data, so it's treated as erased
    /// flash (`0xFF`) or otherwise foreign/corrupt bytes rather than
    /// silently burning `duration` periods of silence per stray entry
    /// (up to 47 * 255 * 30ms = ~6 minutes if the whole tail is `0xFF`).
    ///
    /// A rest re-arms `tx_single_tone_on` at 0Hz rather than calling
    /// `tx_single_tone_off` mid-tune: that function's `key_tx = false`
    /// branch calls `rx_on()`, which switches `REG_STATE` to the full
    /// `STATE_RX_ON` (RX front-end live) -- exactly what caused a burst of
    /// static on every rest before this fix. `tx_single_tone_off` is only
    /// called once, after the whole loop, same as `play_tone_sequence`
    /// does for a plain UI beep (which never audibly hisses): the RX front
    /// end stays off (`REG_STATE = STATE_TONE`) for the tune's entire
    /// duration, not toggled live and back for every pause in it.
    /// Sidetone only (never keys the transmitter) -- called once at boot,
    /// before the radio has entered RX.
    pub fn play_boot_tune(&mut self, syst: &mut SYST, tune: &[(u8, u8); 48]) {
        let mut speaker_connected = false;

        for &(tone_index, duration) in tune {
            if tone_index == 0 && duration == 0 {
                break;
            }
            let hz_div_10 = if tone_index == 0 {
                0 // rest
            } else {
                match BOOT_TUNE_HZ_DIV_10.get((tone_index - 1) as usize) {
                    Some(&hz) => hz,
                    None => break,
                }
            };
            self.fd6818.tx_single_tone_on(syst, hz_div_10, false);
            if !speaker_connected {
                // See `play_tone_sequence`'s doc comment: connect the
                // speaker only after the tone is already configured, not
                // before.
                delay::ms(syst, TONE_SETTLE_MS);
                board::set_speaker_switch(self.gpiob, true);
                speaker_connected = true;
            }
            delay::ms(syst, duration as u32 * BOOT_TUNE_PERIOD_MS);
        }
        // See `play_tone_sequence`'s doc comment for why the speaker is
        // physically disconnected here, before `tx_single_tone_off`.
        board::set_speaker_switch(self.gpiob, false);
        self.fd6818.tx_single_tone_off(syst, false);

        let state = if self.audio_open {
            AfOutState::RxAudio
        } else {
            AfOutState::Mute
        };
        self.fd6818
            .set_af_out(syst, state, self.cfg.wide_band, self.cfg.modulation);
        self.fd6818.set_scramble(syst, self.scramble_level);
        board::set_speaker_switch(self.gpiob, self.audio_open);
    }

    fn play_mdc1200_tone(&mut self, syst: &mut SYST) {
        self.fd6818.enter_mdc1200_mode(syst);
        self.fd6818.mdc1200_tone_tx(syst);
        self.fd6818.exit_mdc1200_mode(syst);
    }

    fn play_tone_sequence(
        &mut self,
        syst: &mut SYST,
        tones: &[(u16, u32)],
        key_tx: bool,
        local_speaker: bool,
    ) {
        let mut speaker_connected = false;
        for &(hz_div_10, duration_ms) in tones {
            self.fd6818.tx_single_tone_on(syst, hz_div_10, key_tx);
            if local_speaker && !speaker_connected {
                delay::ms(syst, TONE_SETTLE_MS);
                board::set_speaker_switch(self.gpiob, true);
                speaker_connected = true;
            }
            delay::ms(syst, duration_ms);
        }
        if local_speaker {
            board::set_speaker_switch(self.gpiob, false);
        }
        self.fd6818.tx_single_tone_off(syst, key_tx);

        let state = if self.audio_open {
            AfOutState::RxAudio
        } else {
            AfOutState::Mute
        };
        self.fd6818
            .set_af_out(syst, state, self.cfg.wide_band, self.cfg.modulation);
        self.fd6818.set_scramble(syst, self.scramble_level);
        if local_speaker {
            board::set_speaker_switch(self.gpiob, self.audio_open);
        }
    }

    pub fn rtone_on(&mut self, syst: &mut SYST, hz_div_10: u16) {
        self.fd6818.tx_single_tone_on(syst, hz_div_10, true);
    }

    pub fn rtone_off(&mut self, syst: &mut SYST) {
        self.fd6818.tx_single_tone_off(syst, true);
        self.fd6818.set_scramble(syst, self.scramble_level);
    }

    pub fn toggle_monitor(&mut self) {
        self.monitor = !self.monitor;
    }

    pub fn is_monitor(&self) -> bool {
        self.monitor
    }

    // squelch / RSSI

    pub fn rssi_open(&self) -> bool {
        self.rssi_open
    }

    pub fn poll_squelch(&mut self, syst: &mut SYST, debounce_ticks: u8) -> bool {
        if self.audio_open && self.fd6818.tail_detected(syst) && !self.monitor {
            self.audio_open = false;
            self.sq_debounce = 0;
            self.fd6818.set_af_out(
                syst,
                AfOutState::Mute,
                self.cfg.wide_band,
                self.cfg.modulation,
            );
            board::set_speaker_switch(self.gpiob, false);
            return false;
        }

        let rssi_open = self.fd6818.squelch_open(syst);

        if rssi_open != self.rssi_open {
            self.rssi_debounce += 1;
            if self.rssi_debounce >= debounce_ticks {
                self.rssi_open = rssi_open;
                self.rssi_debounce = 0;
                if rssi_open {
                    board::set_rx_led(self.gpioa, true);
                }
            }
        } else {
            self.rssi_debounce = 0;
        }

        let tone_ok = match self.cfg.subaudio_rx {
            SubAudio::None => true,
            SubAudio::Ctcss(_) | SubAudio::Dcs { .. } => self.fd6818.subaudio_matched(syst),
        };
        let open = self.monitor || (rssi_open && tone_ok);
        if open != self.audio_open {
            self.sq_debounce += 1;
            if self.sq_debounce >= debounce_ticks {
                self.audio_open = open;
                self.sq_debounce = 0;
                let state = if open {
                    AfOutState::RxAudio
                } else {
                    AfOutState::Mute
                };
                self.fd6818
                    .set_af_out(syst, state, self.cfg.wide_band, self.cfg.modulation);
                board::set_rx_led(self.gpioa, open);
                board::set_speaker_switch(self.gpiob, open);
            }
        } else {
            self.sq_debounce = 0;
        }
        self.audio_open
    }

    pub fn rssi(&mut self, syst: &mut SYST) -> u16 {
        self.fd6818.get_rssi(syst)
    }

    pub fn audio_is_open(&self) -> bool {
        self.audio_open
    }

    // TX / RX state machine
    pub fn enter_rx(&mut self, syst: &mut SYST) {
        self.fd6818.idle(syst);
        self.fd6818.pa_off(syst);
        self.fd6818.set_tx_band_off(syst);
        self.fd6818.power_rx(syst);
        self.fd6818.set_scramble(syst, self.scramble_level);
        let tuned_freq_hz = if self.cfg.modulation == Modulation::Usb {
            self.cfg.freq_hz.saturating_add_signed(self.rit_offset_hz)
        } else {
            self.cfg.freq_hz
        };
        self.fd6818.set_frequency_hz(syst, tuned_freq_hz);
        self.fd6818.set_wide_bandwidth(syst, self.cfg.wide_band);
        self.fd6818
            .set_squelch_level(syst, self.cfg.freq_hz, self.sql_level);
        self.fd6818.enable_rx_subaudio(syst, self.cfg.subaudio_rx);
        self.fd6818.apply_modulation(syst, self.cfg.modulation);
        self.fd6818.rx_on(syst);

        self.fd6818.set_af_out(
            syst,
            AfOutState::Mute,
            self.cfg.wide_band,
            self.cfg.modulation,
        );
        self.audio_open = false;
        self.sq_debounce = 0;
        self.rssi_open = false;
        self.rssi_debounce = 0;
        board::set_rx_led(self.gpioa, false);
        self.fd6818.set_tx_led(syst, false);
        match self.band() {
            Band::Uhf => board::set_rx_band_uhf(self.gpioa),
            Band::Vhf => board::set_rx_band_vhf(self.gpioa),
        }
        board::set_speaker_switch(self.gpiob, false);
        self.fd6818.enter_dtmf_mode(syst, false);
        self.dtmf_rx_len = 0;
        self.dtmf_rx_timeout = 0;
        self.tx_state = false;
    }

    #[must_use]
    pub fn enter_tx(&mut self, syst: &mut SYST) -> bool {
        if !self.tx_allowed.allows(self.cfg.tx_freq_hz) || self.cfg.modulation != Modulation::Fm {
            return false;
        }
        board::set_speaker_switch(self.gpiob, false);
        self.fd6818.rf_off(syst);
        board::set_rx_band_off(self.gpioa);
        self.fd6818.idle(syst);
        self.fd6818.wake(syst);
        self.fd6818.set_frequency_hz(syst, self.cfg.tx_freq_hz);
        self.fd6818.set_wide_bandwidth(syst, self.cfg.wide_band);
        self.fd6818.set_subaudio_tx(syst, self.cfg.subaudio_tx);
        self.fd6818.set_scramble(syst, self.scramble_level);
        self.fd6818.apply_tx_mic_gain(syst);
        self.fd6818.tx_on(syst);
        self.fd6818.pa_enable(syst, self.cfg.power);
        match self.band() {
            Band::Uhf => self.fd6818.set_tx_band_uhf(syst),
            Band::Vhf => self.fd6818.set_tx_band_vhf(syst),
        }
        self.fd6818.set_tx_led(syst, true);
        self.tx_state = true;
        true
    }

    pub fn end_tx(&mut self, syst: &mut SYST) {
        match self.roger_tone {
            RogerTone::Off => {}
            RogerTone::Roger => self.play_roger_tone(syst),
            RogerTone::Mdc1200 => self.play_mdc1200_tone(syst),
        }
        if self.tail_elimination {
            self.fd6818.send_tail(syst, true);
            delay::ms(syst, SEND_TAIL_HOLD_MS);
            self.fd6818.send_tail(syst, false);
        }
        if self.rptrl > 0 {
            delay::ms(syst, self.rptrl as u32 * 100);
        }
        self.enter_rx(syst);
    }

    pub fn rf_sleep(&mut self, syst: &mut SYST) {
        self.fd6818.sleep(syst);
    }

    pub fn tune_search_candidate(&mut self, syst: &mut SYST, freq_hz: u32, uhf_path: bool) {
        self.fd6818.idle(syst);
        self.fd6818.set_frequency_hz(syst, freq_hz);
        self.fd6818.set_wide_bandwidth(syst, true);
        if uhf_path {
            board::set_rx_band_uhf(self.gpioa);
        } else {
            board::set_rx_band_vhf(self.gpioa);
        }
        self.fd6818.rx_on(syst);
    }

    pub fn freq_scan_enable(&mut self, syst: &mut SYST) {
        self.fd6818.freq_scan_enable(syst);
    }

    pub fn freq_scan_disable(&mut self, syst: &mut SYST) {
        self.fd6818.freq_scan_disable(syst);
    }

    pub fn check_freq_scan(&mut self, syst: &mut SYST) -> Option<u32> {
        self.fd6818.check_freq_scan(syst)
    }

    pub fn set_subaudio_scan_filter(&mut self, syst: &mut SYST, enabled: bool) {
        self.fd6818.set_subaudio_scan_filter(syst, enabled);
    }

    pub fn detect_subaudio_raw(&mut self, syst: &mut SYST) -> RawTone {
        self.fd6818.detect_subaudio_raw(syst)
    }

    pub fn correct_measured_freq_word(&self, raw_word: u32) -> u32 {
        self.fd6818.correct_measured_freq_word(raw_word)
    }
}

pub fn ctcss_raw_to_tenths_hz(raw: u16) -> u16 {
    Fd6818::ctcss_raw_to_tenths_hz(raw)
}

pub fn find_standard_ctcss(tenths_hz: u16, table: &[u16]) -> Option<u16> {
    Fd6818::find_standard_ctcss(tenths_hz, table)
}

pub fn find_standard_dcs(raw23: u32, table: &[u16]) -> Option<u16> {
    Fd6818::find_standard_dcs(raw23, table)
}
