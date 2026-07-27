use crate::board;
use crate::delay;
use crate::fd6818::{AfOutState, Fd6818, Power, SubAudio};
use cortex_m::peripheral::SYST;
use kd32f328_pac::{gpioa, gpiof};

/// How long `end_tx()` holds the tail-elimination tone before actually
/// cutting the carrier.
const SEND_TAIL_HOLD_MS: u32 = 300;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Band {
    Vhf,
    Uhf,
}

// TODO: more channel field
// TODO: VFO info
pub struct ChannelConfig {
    pub freq_hz: u32,
    pub wide_band: bool,
    pub power: Power,
    pub sql_level: u8,
    pub subaudio_tx: SubAudio,
    pub subaudio_rx: SubAudio,

    // TODO: move to global settings
    pub tail_elimination: bool,
}

pub struct Radio<'a> {
    fd6818: Fd6818<'a>,
    gpioa: &'a gpioa::RegisterBlock,
    gpiob: &'a gpiof::RegisterBlock,
    cfg: ChannelConfig,

    /// Whether `REG_AF_OUT` is currently routed to `RxAudio` (vs `Mute`).
    /// the chip's own squelch decision (`REG 0x78`) doesn't touch this
    /// register, so it's tracked and driven from here.
    audio_open: bool,
    sq_debounce: u8,
}

impl<'a> Radio<'a> {
    pub fn new(
        fd6818: Fd6818<'a>,
        gpioa: &'a gpioa::RegisterBlock,
        gpiob: &'a gpiof::RegisterBlock,
        cfg: ChannelConfig,
    ) -> Self {
        Radio { fd6818, gpioa, gpiob, cfg, audio_open: false, sq_debounce: 0 }
    }

    pub fn init(&mut self, syst: &mut SYST) {
        self.fd6818.init(syst);
    }

    /// Escape hatch for calibration loading and register-level debug
    pub fn fd6818_mut(&mut self) -> &mut Fd6818<'a> {
        &mut self.fd6818
    }

    fn band(&self) -> Band {
        if self.cfg.freq_hz >= 300_000_000 {
            Band::Uhf
        } else {
            Band::Vhf
        }
    }

    pub fn set_frequency(&mut self, freq_hz: u32) {
        self.cfg.freq_hz = freq_hz;
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

    pub fn set_sql_level(&mut self, syst: &mut SYST, level: u8) {
        self.cfg.sql_level = level;
        self.fd6818.set_squelch_level(syst, self.cfg.freq_hz, level);
    }

    pub fn squelch_open(&mut self, syst: &mut SYST) -> bool {
        self.fd6818.squelch_open(syst)
    }

    pub fn poll_squelch(&mut self, syst: &mut SYST, debounce_ticks: u8) -> bool {
        if self.audio_open && self.fd6818.tail_detected(syst) {
            self.audio_open = false;
            self.sq_debounce = 0;
            self.fd6818.set_af_out(syst, AfOutState::Mute, self.cfg.wide_band);
            return false;
        }

        let rssi_open = self.fd6818.squelch_open(syst);
        let tone_ok = match self.cfg.subaudio_rx {
            SubAudio::None => true,
            SubAudio::Ctcss(_) => self.fd6818.subaudio_matched(syst),
        };
        let open = rssi_open && tone_ok;
        if open != self.audio_open {
            self.sq_debounce += 1;
            if self.sq_debounce >= debounce_ticks {
                self.audio_open = open;
                self.sq_debounce = 0;
                let state = if open { AfOutState::RxAudio } else { AfOutState::Mute };
                self.fd6818.set_af_out(syst, state, self.cfg.wide_band);
            }
        } else {
            self.sq_debounce = 0;
        }
        self.audio_open
    }

    pub fn rssi(&mut self, syst: &mut SYST) -> u16 {
        self.fd6818.get_rssi(syst)
    }

    pub fn enter_rx(&mut self, syst: &mut SYST) {
        self.fd6818.idle(syst);
        self.fd6818.pa_off(syst);
        self.fd6818.set_scramble_off(syst);
        self.fd6818.set_frequency_hz(syst, self.cfg.freq_hz);
        self.fd6818.set_wide_bandwidth(syst, self.cfg.wide_band);
        self.fd6818.set_squelch_level(syst, self.cfg.freq_hz, self.cfg.sql_level);
        self.fd6818.enable_rx_subaudio(syst, self.cfg.subaudio_rx);
        self.fd6818.rx_on(syst);

        // Start muted; `poll_squelch()` is what actually opens audio, once
        // REG 0x78's sq_out flag has had a chance to settle at the new
        // frequency/threshold instead of momentarily passing through
        // whatever the chip read right at retune.
        self.fd6818.set_af_out(syst, AfOutState::Mute, self.cfg.wide_band);
        self.audio_open = false;
        self.sq_debounce = 0;
        match self.band() {
            Band::Uhf => board::set_rx_band_uhf(self.gpioa),
            Band::Vhf => board::set_rx_band_vhf(self.gpioa),
        }
        board::set_speaker_switch(self.gpiob, true);
    }

    pub fn enter_tx(&mut self, syst: &mut SYST) {
        board::set_speaker_switch(self.gpiob, false);
        self.fd6818.rf_off(syst);
        board::set_rx_band_off(self.gpioa);
        self.fd6818.idle(syst);
        self.fd6818.wake(syst);
        self.fd6818.set_frequency_hz(syst, self.cfg.freq_hz);
        self.fd6818.set_wide_bandwidth(syst, self.cfg.wide_band);
        self.fd6818.set_subaudio_tx(syst, self.cfg.subaudio_tx);
        self.fd6818.set_scramble_off(syst);
        self.fd6818.apply_tx_mic_gain(syst);
        self.fd6818.tx_on(syst);
        self.fd6818.pa_enable(syst, self.cfg.power);
        match self.band() {
            Band::Uhf => self.fd6818.set_tx_band_uhf(syst),
            Band::Vhf => self.fd6818.set_tx_band_vhf(syst),
        }
    }

    pub fn end_tx(&mut self, syst: &mut SYST) {
        if self.cfg.tail_elimination {
            self.fd6818.send_tail(syst, true);
            delay::ms(syst, SEND_TAIL_HOLD_MS);
            self.fd6818.send_tail(syst, false);
        }
        self.enter_rx(syst);
    }
}
