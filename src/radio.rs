use crate::board;
use crate::drivers::fd6818::{AfOutState, Fd6818, Power, SubAudio};
use crate::hal::delay;
use cortex_m::peripheral::SYST;
use kd32f328_pac::{gpioa, gpiof};

/// How long `end_tx()` holds the tail-elimination tone before actually
/// cutting the carrier.
const SEND_TAIL_HOLD_MS: u32 = 300;

/// UI key-beep default tone sequence: 1500Hz/80ms then 450Hz/35ms
/// (`hz_div_10`, `duration_ms`) pairs
const BEEP_TONES: [(u16, u32); 2] = [(150, 80), (45, 35)];
/// Roger-beep tone pair: 1000Hz then 850Hz, 80ms each
const ROGER_TONES: [(u16, u32); 2] = [(100, 80), (85, 80)];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Band {
    Vhf,
    Uhf,
}

// TODO: more channel field
// TODO: VFO info
#[derive(Clone, Copy)]
pub struct ChannelConfig {
    pub freq_hz: u32,
    pub tx_freq_hz: u32,
    pub wide_band: bool,
    pub power: Power,
    pub subaudio_tx: SubAudio,
    pub subaudio_rx: SubAudio,
}

pub struct Radio<'a> {
    fd6818: Fd6818<'a>,
    gpioa: &'a gpioa::RegisterBlock,
    gpiob: &'a gpiof::RegisterBlock,
    cfg: ChannelConfig,

    sql_level: u8,
    tail_elimination: bool,
    beeps_enabled: bool,
    roger_beep: bool,
    scramble_level: u8,

    /// Whether `REG_AF_OUT` is currently routed to `RxAudio` (vs `Mute`).
    /// the chip's own squelch decision (`REG 0x78`) doesn't touch this
    /// register, so it's tracked and driven from here.
    audio_open: bool,
    sq_debounce: u8,

    rssi_open: bool,
    rssi_debounce: u8,
}

impl<'a> Radio<'a> {
    pub fn new(
        fd6818: Fd6818<'a>,
        gpioa: &'a gpioa::RegisterBlock,
        gpiob: &'a gpiof::RegisterBlock,
        cfg: ChannelConfig,
    ) -> Self {
        Radio {
            fd6818,
            gpioa,
            gpiob,
            cfg,
            // FIXME: Overridden immediately by the caller once global settings
            // are loaded; these are just placeholder startup values.
            sql_level: 3,
            tail_elimination: true,
            beeps_enabled: true,
            roger_beep: false,
            scramble_level: 0,
            audio_open: false,
            sq_debounce: 0,
            rssi_open: false,
            rssi_debounce: 0,
        }
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

    pub fn set_sql_level(&mut self, syst: &mut SYST, level: u8) {
        self.sql_level = level;
        self.fd6818.set_squelch_level(syst, self.cfg.freq_hz, level);
    }

    pub fn set_tail_elimination(&mut self, enabled: bool) {
        self.tail_elimination = enabled;
    }

    pub fn set_beeps_enabled(&mut self, enabled: bool) {
        self.beeps_enabled = enabled;
    }

    pub fn set_roger_beep(&mut self, enabled: bool) {
        self.roger_beep = enabled;
    }

    /// Voice-inversion scramble group, 0 (off) - 3. Re-applied on every
    /// `enter_rx`/`enter_tx` and after any tone burst, since it shares
    /// `REG_TONE_FREQ` with the single-tone oscillator and would otherwise
    /// go silently stale once a beep/roger tone clobbers that register.
    pub fn set_scramble_level(&mut self, syst: &mut SYST, level: u8) {
        self.scramble_level = level;
        self.fd6818.set_scramble(syst, level);
    }

    /// Transmits a DTMF digit string over RF. Caller is responsible for
    /// having TX already keyed; PTT state is left untouched.
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
    }

    /// Local-only UI key-beep: doesn't key the transmitter.
    pub fn play_beep(&mut self, syst: &mut SYST) {
        if !self.beeps_enabled || self.audio_open {
            return;
        }
        board::set_speaker_switch(self.gpiob, true);
        self.play_tone_sequence(syst, &BEEP_TONES, false);
        // Back to the squelch-driven state (closed, or the beep wouldn't
        // have played) so the VOX mic preamp stays alive.
        board::set_speaker_switch(self.gpiob, self.audio_open);
    }

    /// Two-tone "roger beep", transmitted over RF right after PTT release
    /// called from `end_tx()`, before tail-elimination/actually dropping
    /// the carrier
    fn play_roger_tone(&mut self, syst: &mut SYST) {
        self.play_tone_sequence(syst, &ROGER_TONES, true);
    }

    fn play_tone_sequence(&mut self, syst: &mut SYST, tones: &[(u16, u32)], key_tx: bool) {
        for &(hz_div_10, duration_ms) in tones {
            self.fd6818.tx_single_tone_on(syst, hz_div_10, key_tx);
            delay::ms(syst, duration_ms);
        }
        self.fd6818.tx_single_tone_off(syst, key_tx);

        // Tone playback silently overwrote REG_AF_OUT; restore it to
        // whatever `poll_squelch()`'s last decision actually was, since that
        // decision (`self.audio_open`) may not have changed and so
        // wouldn't otherwise get re-applied.
        let state = if self.audio_open {
            AfOutState::RxAudio
        } else {
            AfOutState::Mute
        };
        self.fd6818.set_af_out(syst, state, self.cfg.wide_band);
        self.fd6818.set_scramble(syst, self.scramble_level);
    }

    /// Starts the repeater-access tone (`hz_div_10`, e.g. 175 = 1750Hz)
    /// going out over RF. Only meaningful while TX is already keyed,
    /// the tone replaces the mic path until `rtone_off`.
    pub fn rtone_on(&mut self, syst: &mut SYST, hz_div_10: u16) {
        self.fd6818.tx_single_tone_on(syst, hz_div_10, true);
    }

    /// Ends the access-tone burst and returns to ordinary TX audio. The
    /// scrambler shares `REG_TONE_FREQ` with the tone oscillator, so it has
    /// to be restored after the tone register was clobbered.
    pub fn rtone_off(&mut self, syst: &mut SYST) {
        self.fd6818.tx_single_tone_off(syst, true);
        self.fd6818.set_scramble(syst, self.scramble_level);
    }

    pub fn rssi_open(&self) -> bool {
        self.rssi_open
    }

    pub fn poll_squelch(&mut self, syst: &mut SYST, debounce_ticks: u8) -> bool {
        if self.audio_open && self.fd6818.tail_detected(syst) {
            self.audio_open = false;
            self.sq_debounce = 0;
            self.fd6818
                .set_af_out(syst, AfOutState::Mute, self.cfg.wide_band);
            board::set_speaker_switch(self.gpiob, false);
            return false;
        }

        let rssi_open = self.fd6818.squelch_open(syst);

        if rssi_open != self.rssi_open {
            self.rssi_debounce += 1;
            if self.rssi_debounce >= debounce_ticks {
                self.rssi_open = rssi_open;
                self.rssi_debounce = 0;
                board::set_rx_led(self.gpioa, self.rssi_open);
            }
        } else {
            self.rssi_debounce = 0;
        }

        let tone_ok = match self.cfg.subaudio_rx {
            SubAudio::None => true,
            SubAudio::Ctcss(_) | SubAudio::Dcs { .. } => self.fd6818.subaudio_matched(syst),
        };
        let open = rssi_open && tone_ok;
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
                self.fd6818.set_af_out(syst, state, self.cfg.wide_band);
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

    pub fn enter_rx(&mut self, syst: &mut SYST) {
        self.fd6818.idle(syst);
        self.fd6818.pa_off(syst);
        self.fd6818.set_tx_band_off(syst);
        self.fd6818.power_rx(syst);
        self.fd6818.set_scramble(syst, self.scramble_level);
        self.fd6818.set_frequency_hz(syst, self.cfg.freq_hz);
        self.fd6818.set_wide_bandwidth(syst, self.cfg.wide_band);
        self.fd6818
            .set_squelch_level(syst, self.cfg.freq_hz, self.sql_level);
        self.fd6818.enable_rx_subaudio(syst, self.cfg.subaudio_rx);
        self.fd6818.rx_on(syst);

        // Start muted; `poll_squelch()` is what actually opens audio, once
        // REG 0x78's sq_out flag has had a chance to settle at the new
        // frequency/threshold instead of momentarily passing through
        // whatever the chip read right at retune.
        self.fd6818
            .set_af_out(syst, AfOutState::Mute, self.cfg.wide_band);
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
    }

    pub fn enter_tx(&mut self, syst: &mut SYST) {
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
    }

    pub fn end_tx(&mut self, syst: &mut SYST) {
        if self.roger_beep {
            self.play_roger_tone(syst);
        }
        if self.tail_elimination {
            self.fd6818.send_tail(syst, true);
            delay::ms(syst, SEND_TAIL_HOLD_MS);
            self.fd6818.send_tail(syst, false);
        }
        self.enter_rx(syst);
    }
}
