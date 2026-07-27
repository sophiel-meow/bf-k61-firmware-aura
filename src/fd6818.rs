use crate::board;
use crate::delay;
use cortex_m::peripheral::SYST;
use kd32f328_pac::gpiof;

const BIT_DELAY_US: u32 = 5;

const REG_FREQ_LO: u8 = 0x38;
const REG_FREQ_HI: u8 = 0x39;
const REG_STATE: u8 = 0x30; // REG_03H in datasheet
const REG_BANDWIDTH: u8 = 0x43;

/// REG 0x30 is the master block-enable word (datasheet prints it as
/// "REG_03H", but the bit layout matches these values bit-for-bit):
///
/// | bit     | field                                        |
/// |---------|----------------------------------------------|
/// | [15]    | vco_cal_en                                   |
/// | [14]    | pabias_en (voltage itself is REG_PA_BIAS)    |
/// | [13:10] | rxlink_en (LNA, MIXER, FILTER, ADC)          |
/// | [9]     | afout_en — 1 = enable AFOUT DAC              |
/// | [8:4]   | pll_en                                       |
/// | [3]     | padrv_en                                     |
/// | [2]     | micin_en — 1 = enable MICIN ADC              |
/// | [1]     | txon                                         |
/// | [0]     | rxon                                         |
///
const STATE_RX_ON: u16 = 0xBFF1;    // 1 0 1111 1 11111 0001
const STATE_TX_ON: u16 = 0xC1FE;    // 1 1 0000 0 11111 1110

const BANDWIDTH_WIDE: u16 = 0x3028;
const BANDWIDTH_NARROW: u16 = 0x4048;

const REG_SOFT_RESET: u8 = 0x00;
const REG_POWER: u8 = 0x37;
const REG_RXAGC_1: u8 = 0x13;
const REG_RXAGC_2: u8 = 0x12;
const REG_RXAGC_3: u8 = 0x11;
const REG_RXAGC_4: u8 = 0x10;
const REG_RXAGC_5: u8 = 0x14;
const REG_RXAGC_6: u8 = 0x49;
const REG_RXAGC_7: u8 = 0x7B;
/// PA bias output voltage: [3:0] pabias_out, 0000=1.3V .. 1111=2.8V.
const REG_PA_BIAS: u8 = 0x19;
const REG_DEVIATION: u8 = 0x40;
const REG_MIC_SENS: u8 = 0x7D;
const REG_VOLUME: u8 = 0x48;
const REG_PLL_VCO_BIAS: u8 = 0x1F;
const REG_FIXED_3E: u8 = 0x3E;
const REG_ANTI_SPUR: u8 = 0x77;
const REG_OOB_NOISE: u8 = 0x4F;
const REG_DISTORTION: u8 = 0x26;
const REG_RSSI: u8 = 0x67;

const REG_UNKNOWN_2A: u8 = 0x2A;
const REG_UNKNOWN_21: u8 = 0x21;
const REG_SCRAMBLE: u8 = 0x31;
const REG_CTCSS: u8 = 0x51;
/// "CTCSS0" tone bank (the one actually transmitted/decoded); [13]=1
/// selects a second bank always held at the fixed 55.1Hz tail-elimination
const REG_SUBAUDIO_FREQ: u8 = 0x07;
const REG_SUBAUDIO_THRESH: u8 = 0x52;
const SUBAUDIO_THRESH_VALUE: u16 = 0x0292;
/// REG 0x78: RSSI-based squelch threshold, 0.5dB/step.
/// [15:8] rssi_sq_th_in (squelch opens), [7:0] rssi_sq_th_out (squelch closes).
const REG_SQUELCH: u8 = 0x78;
const REG_STATUS: u8 = 0x0C;
const STATUS_SQ_OPEN: u16 = 0x0002;
const STATUS_SUBAUDIO_MATCH: u16 = 0x0400;
const STATUS_TAIL_DETECTED: u16 = 0x0800;

const SUBAUDIO_TAIL_WORD: u16 = 0x01CD | 0x2000;
/// REG 0x07 **bank0** (bit13=0), same 55.1Hz tone as the bank1 constant
/// above but written to the other bank: `send_tail()` briefly overwrites
/// whatever the channel's own CTCSS tone was with this, rather than
/// running a second tone alongside it.
const SEND_TAIL_WORD: u16 = 0x0471;

/// Squelch-level base RSSI thresholds, index = squelch level 0..9 (0 =
/// always open).
const SQL_TH_IN: [u8; 10] = [0, 89, 91, 93, 95, 97, 99, 102, 105, 107];
/// lower than SQL_TH_IN for closing to prevent chattering
const SQL_TH_OUT: [u8; 10] = [0, 87, 89, 91, 93, 95, 97, 99, 102, 105];

const SQL_OFFSET_U_400: [u8; 16] = [4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 0, 0, 0];
const SQL_OFFSET_V_136: [u8; 10] = [4, 4, 4, 4, 4, 4, 4, 4, 2, 2];
const SQL_OFFSET_V_200: [u8; 16] = [0; 16];

/// REG 0x33: chip-internal GPIO used to steer the PA driver between the
/// VHF and UHF output paths. bits[15:8] = output-enable-bar (active low,
/// per bit), bits[7:0] = output value (per bit). GPIO2=bit2, GPIO3=bit3.
const REG_GPIO: u8 = 0x33;
const GPIO2: u16 = 0x0004;
const GPIO3: u16 = 0x0008;

/// REG 0x36: PA enable/gain register. REG_28H in Datasheet.
///
/// | bits  | field                                                     |
/// |-------|-----------------------------------------------------------|
/// | [15:8]| APC target (calibrated per-radio/per-band, from flash)    |
/// | [7]   | PACTL output enable, 1 = on                               |
/// | [6]   | reserved                                                  |
/// | [5:3] | `padrv_gain`, 111 = max .. 000 = min                      |
/// | [2:0] | `pa_gain_vreg`, 111 = max .. 000 = min                    |
const REG_PA: u8 = 0x36;
/// PACTL disabled; gain fields left at max (they are don't-care while the
/// output is gated off).
const PA_OFF_VALUE: u16 = 0x007F;
/// PACTL on, `padrv_gain` = 7, `pa_gain_vreg` = 7 — max driver gain
/// (datasheet's power table: 8.33dB). Used for the high and mid power
/// levels, where the level itself is set purely by the APC target.
const PA_GAIN_HIGH: u16 = 0x00FF;
/// PACTL on, `padrv_gain` = 2, `pa_gain_vreg` = 7 — 5.69dB, i.e. 2.6dB
/// below max drive. This is what the original pairs with the low-power APC
/// table; low power is the one level that backs off the driver itself.
const PA_GAIN_LOW: u16 = 0x00D7;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Low,
    Mid,
    High,
}

/// Flash base address of each power level's APC target table
const PA_TABLE_BASE: [u32; 3] = [0xF000, 0xF040, 0xF080]; // High, Mid, Low

/// DTMF Goertzel-style coefficient table.
/// Same physical register (0x09) is
/// written 16 times; the coefficient's table index is packed into the top
/// 4 bits of each write and the coefficient itself into the low 12 bits.
const REG_DTMF_COEF: u8 = 0x09;
const DTMF_COEF_TABLE: [u16; 16] = [
    0x006F, 0x106B, 0x2067, 0x3062, 0x4050, 0x5047, 0x603A, 0x702C, 0x8041, 0x9037, 0xA025,
    0xB017, 0xC0E4, 0xD0CB, 0xE0B5, 0xF09F,
];

const REG_FSK_BAUD: u8 = 0x72;
const REG_FSK_5C: u8 = 0x5C;
const REG_FSK_5D: u8 = 0x5D;
const FSK_BAUD_1200: u16 = 0x3065; // <<1 for 2400
const FSK_5C_VALUE: u16 = 0x5665;
/// `(FSK_LEN * 2 - 1) << 8`, FSK_LEN = 8
const FSK_5D_VALUE: u16 = 0x0F00; // 0d15<<8 =0b0000_1111 << 8 = 0x0F00

const REG_AF_TX_3K: u8 = 0x74;
const REG_AF_TX_300_D1: u8 = 0x44;
const REG_AF_TX_300_D2: u8 = 0x45;
const REG_AF_RX_3K: u8 = 0x75;
const REG_AF_RX_300_D1: u8 = 0x54;
const REG_AF_RX_300_D2: u8 = 0x55;

/// REG 0x47: audio-out routing. Base value 0x6040 ORed with one of the
/// state bitfields below; REG_VOLUME (0x48) is switched alongside it since
/// beep tone uses a fixed volume independent of the wideband/narrowband
/// calibration values.
/// FIXME: check datasheet for real value
const REG_AF_OUT: u8 = 0x47;
const AF_OUT_BASE: u16 = 0x6040;
const AF_STATE_MUTE: u16 = 0x0000;
const AF_STATE_RX_AUDIO: u16 = 0x0100;
const AF_STATE_RX_ALARM_TONE: u16 = 0x0200;
const AF_STATE_BEEP: u16 = 0x0300;
const AF_STATE_CTC_DCS_TEST: u16 = 0x0600;
const AF_STATE_FSK_TEST: u16 = 0x0800;
/// Beep tone volume is independent of the calibrated wideband/narrowband
/// levels: fixed digital gain 10, analog gain 2.
const AF_OUT_BEEP_VOLUME: u16 = 0xB800 | (10 << 4) | 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AfOutState {
    Mute,
    RxAudio,
    RxAlarmTone,
    Beep,
    CtcDcsTest,
    FskTest,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SubAudio {
    None,
    /// Analog CTCSS tone, in tenths of Hz (e.g. `1000` = 100.0Hz).
    Ctcss(u16),
}

/// 26 MHz xtal calibration table
/// The index is derived from the XTAL_ADJUST byte in spi flash
/// Entry 8 corresponds to the nominal frequency (no correction)
const XTAL_ADJUST_TABLE: [u32; 17] = [
    40, 35, 30, 25, 20, 15, 10, 5, 0, 5, 10, 15, 20, 25, 30, 35, 40,
];
const XTAL_ADJUST_ZERO_POINT: u8 = 8;

const POWER_UP_VALUE: u16 = 0x1D0F;
/// dev_sh=0x4, dev_lvl=0xE0, GAIN = (256 + 224) >> 4 = 30.
const DEVIATION_VALUE: u16 = 0x04E0;
const MIC_SENS_VALUE: u16 = 0xE952;
const VOL_GAIN: u16 = 59;
const DAC_GAIN: u16 = 15;

pub struct Fd6818<'a> {
    gpiob: &'a gpiof::RegisterBlock,
    xtal_adjust: u8,
    vol_wideband: u8,
    vol_narrowband: u8,
    mic_mod_depth: u8,
    cts_mod_depth: u8,
    pa_target: u8,
}

impl<'a> Fd6818<'a> {
    pub fn new(gpiob: &'a gpiof::RegisterBlock) -> Self {
        Fd6818 {
            gpiob,
            xtal_adjust: XTAL_ADJUST_ZERO_POINT,
            vol_wideband: 25,
            vol_narrowband: 25,
            mic_mod_depth: 0,
            cts_mod_depth: 0,
            pa_target: 100,
        }
    }

    /// xtal calibration byte in flash (0xF210+6)
    pub fn set_xtal_adjust(&mut self, value: u8) {
        self.xtal_adjust = value;
    }

    /// Flash address holding the calibrated APC target byte for `freq_hz`
    /// at the given power level.
    /// Only U_400/V_136/V_200 bands are actually calibrated
    pub fn pa_target_addr(freq_hz: u32, power: Power) -> Option<u32> {
        let base = PA_TABLE_BASE[power as usize];
        let mhz = freq_hz / 1_000_000;
        if freq_hz >= 400_000_000 {
            Some(base + (mhz - 400) / 10)
        } else if freq_hz >= 200_000_000 {
            Some(base + 0x20 + (mhz - 200) / 5)
        } else if freq_hz >= 130_000_000 {
            let idx = ((mhz - 130) / 3).min(15);
            Some(base + 0x10 + idx)
        } else {
            None
        }
    }

    /// Calibrated APC target byte, read from flash at the address
    /// `pa_target_addr()` gives for the current frequency and power level.
    pub fn set_pa_calibration(&mut self, target: u8) {
        self.pa_target = target;
    }

    fn set_gpio_bit(&mut self, syst: &mut SYST, gpiox: u16, high: bool) {
        let mut val = self.read_reg(syst, REG_GPIO);
        val &= !(gpiox << 8); // clear enable-bar: enable this GPIO's output
        if high {
            val |= gpiox;
        } else {
            val &= !gpiox;
        }
        self.write_reg(syst, REG_GPIO, val);
    }

    /// Routes the PA driver to the UHF path (350/400MHz bands).
    pub fn set_tx_band_uhf(&mut self, syst: &mut SYST) {
        self.set_gpio_bit(syst, GPIO3, false);
        self.set_gpio_bit(syst, GPIO2, true);
    }

    /// Routes the PA driver to the VHF path (136/200MHz bands).
    pub fn set_tx_band_vhf(&mut self, syst: &mut SYST) {
        self.set_gpio_bit(syst, GPIO2, false);
        self.set_gpio_bit(syst, GPIO3, true);
    }

    /// Enables the PA stage at the calibrated APC target for `power`,
    /// pairing it with that level's driver gain
    pub fn pa_enable(&mut self, syst: &mut SYST, power: Power) {
        let gain = match power {
            Power::Low => PA_GAIN_LOW,
            Power::Mid | Power::High => PA_GAIN_HIGH,
        };
        let value = ((self.pa_target as u16) << 8) | gain;
        self.write_reg(syst, REG_PA, value);
    }

    pub fn pa_off(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_PA, PA_OFF_VALUE);
    }

    /// called on every PTT
    /// The caller is responsible for `RfOff()`'s other two effects: the
    /// speaker amp and the RX front-end band pins (`board::set_rx_band_off`).
    pub fn rf_off(&mut self, syst: &mut SYST) {
        self.set_af_out(syst, AfOutState::Mute, true);
        self.set_gpio_bit(syst, GPIO2, false);
        self.set_gpio_bit(syst, GPIO3, false);
    }

    /// Calibration bytes from the flash block at 0xF210 : offset 0 is mic
    /// modulation depth, offset 1 is CTCSS modulation depth, offsets 3/4 are
    /// wideband/narrowband AF volume (max 31 each).
    pub fn set_audio_calibration(
        &mut self,
        mic_mod_depth: u8,
        cts_mod_depth: u8,
        vol_wideband: u8,
        vol_narrowband: u8,
    ) {
        self.mic_mod_depth = mic_mod_depth;
        self.cts_mod_depth = cts_mod_depth;
        self.vol_wideband = vol_wideband;
        self.vol_narrowband = vol_narrowband;
    }

    /// Selects what REG 0x47 routes to the audio-out pin and switches
    /// REG_VOLUME (0x48) alongside it. `wide` selects which calibrated
    /// volume level applies when the state isn't a fixed-volume tone.
    pub fn set_af_out(&mut self, syst: &mut SYST, state: AfOutState, wide: bool) {
        let state_bits = match state {
            AfOutState::Mute => AF_STATE_MUTE,
            AfOutState::RxAudio => AF_STATE_RX_AUDIO,
            AfOutState::RxAlarmTone => AF_STATE_RX_ALARM_TONE,
            AfOutState::Beep => AF_STATE_BEEP,
            AfOutState::CtcDcsTest => AF_STATE_CTC_DCS_TEST,
            AfOutState::FskTest => AF_STATE_FSK_TEST,
        };
        self.write_reg(syst, REG_AF_OUT, AF_OUT_BASE | state_bits);

        let vol_reg = if state == AfOutState::Beep {
            AF_OUT_BEEP_VOLUME
        } else {
            let vol = (if wide { self.vol_wideband } else { self.vol_narrowband } & 0x3F) as u16;
            0x8000 | (vol << 4) | DAC_GAIN
        };
        self.write_reg(syst, REG_VOLUME, vol_reg);
    }

    pub fn apply_tx_mic_gain(&mut self, syst: &mut SYST) {
        let gain = (self.mic_mod_depth % 32) as u16;
        self.write_reg(syst, REG_MIC_SENS, (MIC_SENS_VALUE & 0xFFE0) | gain);
    }

    /// Configures the transmitted sub-audible tone
    ///
    /// `REG_CTCSS` (0x51): bit15 `subau_en`, bit12 `ctc_dcs_sel` (1=CTCSS),
    /// bits[6:0] `subau_gain`
    /// TODO: DCS
    pub fn set_subaudio_tx(&mut self, syst: &mut SYST, sub: SubAudio) {
        match sub {
            SubAudio::None => {
                self.write_reg(syst, REG_CTCSS, 0x0000);
            }
            SubAudio::Ctcss(tenths_hz) => {
                let gain = (self.cts_mod_depth & 0x7F) as u16;
                self.write_reg(syst, REG_CTCSS, 0x9000 | gain);
                // datasheet: ctc_freq = freq_Hz * 2^27/6500000; tenths_hz
                // is freq*10, so this is (tenths_hz/10)*20.6489 rearranged
                // to avoid the intermediate fraction.
                let word = ((tenths_hz as u32 * 206_489) / 100_000) as u16;
                self.write_reg(syst, REG_SUBAUDIO_FREQ, word & 0x1FFF);

                self.write_reg(syst, REG_SUBAUDIO_FREQ, SUBAUDIO_TAIL_WORD);
                self.write_reg(syst, REG_SUBAUDIO_THRESH, SUBAUDIO_THRESH_VALUE);
            }
        }
    }

    pub fn send_tail(&mut self, syst: &mut SYST, on: bool) {
        if on {
            let gain = (self.cts_mod_depth & 0x7F) as u16;
            self.write_reg(syst, REG_CTCSS, 0x9000 | gain);
            self.write_reg(syst, REG_SUBAUDIO_FREQ, SEND_TAIL_WORD);
        } else {
            self.write_reg(syst, REG_CTCSS, 0x0000);
        }
    }

    pub fn enable_rx_subaudio(&mut self, syst: &mut SYST, sub: SubAudio) {
        let primary_tenths_hz = match sub {
            SubAudio::Ctcss(tenths_hz) => tenths_hz,
            SubAudio::None => 551, // 55.1Hz — same as the fixed tail tone
        };
        let gain = (self.cts_mod_depth & 0x7F) as u16;
        self.write_reg(syst, REG_CTCSS, 0x9000 | gain);
        let primary_word = ((primary_tenths_hz as u32 * 206_489) / 100_000) as u16;
        self.write_reg(syst, REG_SUBAUDIO_FREQ, primary_word & 0x1FFF);
        self.write_reg(syst, REG_SUBAUDIO_FREQ, SUBAUDIO_TAIL_WORD);
        self.write_reg(syst, REG_SUBAUDIO_THRESH, SUBAUDIO_THRESH_VALUE);
    }

    pub fn tail_detected(&mut self, syst: &mut SYST) -> bool {
        self.read_reg(syst, REG_STATUS) & STATUS_TAIL_DETECTED != 0
    }

    pub fn subaudio_matched(&mut self, syst: &mut SYST) -> bool {
        self.read_reg(syst, REG_STATUS) & STATUS_SUBAUDIO_MATCH != 0
    }

    pub fn set_squelch_level(&mut self, syst: &mut SYST, freq_hz: u32, level: u8) {
        let level = (level as usize).min(SQL_TH_IN.len() - 1);
        let offset = Self::squelch_offset(freq_hz);
        let th_in = SQL_TH_IN[level].saturating_sub(offset);
        let th_out = SQL_TH_OUT[level].saturating_sub(offset);
        self.write_reg(syst, REG_SQUELCH, ((th_in as u16) << 8) | th_out as u16);
    }

    fn squelch_offset(freq_hz: u32) -> u8 {
        let mhz = freq_hz / 1_000_000;
        if freq_hz >= 400_000_000 {
            let idx = (((mhz - 400) / 10) as usize).min(SQL_OFFSET_U_400.len() - 1);
            SQL_OFFSET_U_400[idx]
        } else if freq_hz >= 200_000_000 {
            let idx = (((mhz - 200) / 5) as usize).min(SQL_OFFSET_V_200.len() - 1);
            SQL_OFFSET_V_200[idx]
        } else if freq_hz >= 130_000_000 {
            let idx = (((mhz - 130) / 5) as usize).min(SQL_OFFSET_V_136.len() - 1);
            SQL_OFFSET_V_136[idx]
        } else {
            0
        }
    }

    /// REG 0x0C bit1 (`sq_out`, read-only): true while squelch is open.
    pub fn squelch_open(&mut self, syst: &mut SYST) -> bool {
        self.read_reg(syst, REG_STATUS) & STATUS_SQ_OPEN != 0
    }

    pub fn wake(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_POWER, POWER_UP_VALUE);
    }

    pub fn set_scramble_off(&mut self, syst: &mut SYST) {
        let scramble = self.read_reg(syst, REG_SCRAMBLE) & !0x0002;
        self.write_reg(syst, REG_SCRAMBLE, scramble);

        let dev = self.read_reg(syst, REG_DEVIATION) & 0xF000;
        self.write_reg(syst, REG_DEVIATION, dev | DEVIATION_VALUE);
    }

    /// (d1, d2) register payload for one AF response trim step.
    /// `db` is a per-radio calibration byte: 0 = default, 1..4 = +1..+4dB,
    /// 5..7 = -1..-3dB. `d2` is unused (0) for the 3kHz corner, which is a
    /// single-register trim.
    fn af_response_coeffs(f3k: bool, db: u8) -> (u16, u16) {
        if f3k {
            let d1 = match db {
                1 => 0xE61C,
                2 => 0xDF22,
                3 => 0xD42D,
                4 => 0xCC35,
                5 => 0xFA02,
                6 => 0xFCFA,
                7 => 0xFEF0,
                _ => 0xF50B,
            };
            (d1, 0)
        } else {
            match db {
                1 => (0x8F90, 0x31F3),
                2 => (0x8F46, 0x31E7),
                3 => (0x8ED8, 0x3232),
                4 => (0x8D8F, 0x3359),
                5 => (0x91C1, 0x3040),
                6 => (0x920B, 0x3010),
                7 => (0x935A, 0x2EFF),
                _ => (0x9009, 0x31A9),
            }
        }
    }

    /// `tx`=true for the transmit path, false for receive;
    /// `f3k`=true for the 3kHz corner, false for the 300Hz corner.
    pub fn set_af_response(&mut self, syst: &mut SYST, tx: bool, f3k: bool, db: u8) {
        let (d1, d2) = Self::af_response_coeffs(f3k, db);
        match (tx, f3k) {
            (true, true) => self.write_reg(syst, REG_AF_TX_3K, d1),
            (true, false) => {
                self.write_reg(syst, REG_AF_TX_300_D1, d1);
                self.write_reg(syst, REG_AF_TX_300_D2, d2);
            }
            (false, true) => self.write_reg(syst, REG_AF_RX_3K, d1),
            (false, false) => {
                self.write_reg(syst, REG_AF_RX_300_D1, d1);
                self.write_reg(syst, REG_AF_RX_300_D2, d2);
            }
        }
    }

    pub fn apply_af_calibration(
        &mut self,
        syst: &mut SYST,
        af_rx_300hz: u8,
        af_rx_3khz: u8,
        af_tx_300hz: u8,
        af_tx_3khz: u8,
    ) {
        self.set_af_response(syst, false, false, af_rx_300hz & 0x07);
        self.set_af_response(syst, false, true, af_rx_3khz & 0x07);
        self.set_af_response(syst, true, false, af_tx_300hz & 0x07);
        self.set_af_response(syst, true, true, af_tx_3khz & 0x07);
    }

    fn write_bits(&mut self, syst: &mut SYST, byte: u8) {
        let mut mask: u8 = 0x80;
        while mask != 0 {
            board::set_fd6818_sck(self.gpiob, false);
            board::set_fd6818_sda(self.gpiob, byte & mask != 0);
            delay::us(syst, BIT_DELAY_US);
            board::set_fd6818_sck(self.gpiob, true);
            delay::us(syst, BIT_DELAY_US);
            mask >>= 1;
        }
    }

    fn read_bits(&mut self, syst: &mut SYST) -> u16 {
        let mut data: u16 = 0;
        for _ in 0..16 {
            data <<= 1;
            board::set_fd6818_sck(self.gpiob, true);
            if board::read_fd6818_sda(self.gpiob) {
                data |= 1;
            }
            delay::us(syst, BIT_DELAY_US);
            board::set_fd6818_sck(self.gpiob, false);
            delay::us(syst, BIT_DELAY_US);
        }
        data
    }

    /// write 16bit reg
    pub fn write_reg(&mut self, syst: &mut SYST, addr: u8, value: u16) {
        board::set_fd6818_scn(self.gpiob, false);
        delay::us(syst, BIT_DELAY_US);
        self.write_bits(syst, addr & 0x7F);
        self.write_bits(syst, (value >> 8) as u8);
        self.write_bits(syst, value as u8);
        board::set_fd6818_scn(self.gpiob, true);
        delay::us(syst, BIT_DELAY_US);
        board::set_fd6818_sck(self.gpiob, false);
    }

    /// read 16bit reg
    pub fn read_reg(&mut self, syst: &mut SYST, addr: u8) -> u16 {
        board::set_fd6818_scn(self.gpiob, false);
        delay::us(syst, BIT_DELAY_US);
        self.write_bits(syst, addr | 0x80);
        board::set_fd6818_sck(self.gpiob, false);
        delay::us(syst, BIT_DELAY_US);

        board::set_fd6818_sda_input(self.gpiob);
        delay::us(syst, BIT_DELAY_US);
        let data = self.read_bits(syst);
        board::set_fd6818_sda_output(self.gpiob);

        board::set_fd6818_scn(self.gpiob, true);
        delay::us(syst, BIT_DELAY_US);
        board::set_fd6818_sda(self.gpiob, false);
        board::set_fd6818_sck(self.gpiob, false);

        data
    }

    /// set freq (Hz, min step 10Hz) with calibration
    /// delta = word * table[idx] / 1e7
    pub fn set_frequency_hz(&mut self, syst: &mut SYST, freq_hz: u32) {
        let mut word = freq_hz / 10;

        let idx = if self.xtal_adjust > 16 {
            XTAL_ADJUST_ZERO_POINT
        } else {
            self.xtal_adjust
        } as usize;
        let adjust = word * XTAL_ADJUST_TABLE[idx] / 10_000_000;
        if idx as u8 > XTAL_ADJUST_ZERO_POINT {
            word += adjust;
        } else {
            word -= adjust;
        }

        self.write_reg(syst, REG_FREQ_LO, word as u16);
        self.write_reg(syst, REG_FREQ_HI, (word >> 16) as u16);
    }

    /// REG 0x67[8:1]：RSSI
    pub fn get_rssi(&mut self, syst: &mut SYST) -> u16 {
        (self.read_reg(syst, REG_RSSI) & 0x01FF) >> 1
    }

    pub fn set_wide_bandwidth(&mut self, syst: &mut SYST, wide: bool) {
        let value = if wide { BANDWIDTH_WIDE } else { BANDWIDTH_NARROW };
        self.write_reg(syst, REG_BANDWIDTH, value);
    }

    const STATE_PREFIX_WIDE: u16 = 0x0200;

    pub fn rx_on(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, Self::STATE_PREFIX_WIDE);
        self.write_reg(syst, REG_STATE, STATE_RX_ON);
    }

    pub fn tx_on(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, Self::STATE_PREFIX_WIDE);
        self.write_reg(syst, REG_STATE, STATE_TX_ON);
    }

    pub fn idle(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, Self::STATE_PREFIX_WIDE);
    }

    pub fn init(&mut self, syst: &mut SYST) {
        // soft reset
        self.write_reg(syst, REG_SOFT_RESET, 0x8000);
        self.write_reg(syst, REG_SOFT_RESET, 0x0000);

        // power up
        self.write_reg(syst, REG_POWER, POWER_UP_VALUE);

        // RX AGC
        self.write_reg(syst, REG_RXAGC_1, 0x03BE);
        self.write_reg(syst, REG_RXAGC_2, 0x037B);
        self.write_reg(syst, REG_RXAGC_3, 0x027B);
        self.write_reg(syst, REG_RXAGC_4, 0x007A);
        self.write_reg(syst, REG_RXAGC_5, 0x0019);
        self.write_reg(syst, REG_RXAGC_6, 0x2A38);
        self.write_reg(syst, REG_RXAGC_7, 0x8420);

        // PA bias output: low nibble 0x1 ~= 1.4V
        self.write_reg(syst, REG_PA_BIAS, 0x1041);

        self.write_reg(syst, REG_UNKNOWN_2A, 0x4F18);

        // DTMF Goertzel coefficient table (decoder itself not implemented yet)
        for &value in DTMF_COEF_TABLE.iter() {
            self.write_reg(syst, REG_DTMF_COEF, value);
        }

        self.write_reg(syst, REG_UNKNOWN_21, 0x06D8);

        // FSK: 1200bps, tx length
        self.write_reg(syst, REG_FSK_BAUD, FSK_BAUD_1200);
        self.write_reg(syst, REG_FSK_5C, FSK_5C_VALUE);
        self.write_reg(syst, REG_FSK_5D, FSK_5D_VALUE);

        // REG_40H
        // [15:13] reserved,
        // [12] dev_en,
        // [11:8] dev_sh (coarse, 0000=max..1111=min),
        // [7:0] dev_lvl (fine, 0=min..255=max);
        // GAIN = (256 + dev_lvl) >> dev_sh.
        let temp = self.read_reg(syst, REG_DEVIATION) & 0xF000;
        self.write_reg(syst, REG_DEVIATION, temp | DEVIATION_VALUE);

        let scramble = self.read_reg(syst, REG_SCRAMBLE) & !0x0002;
        self.write_reg(syst, REG_SCRAMBLE, scramble);

        // mic sensitivity, vol.
        self.write_reg(syst, REG_MIC_SENS, MIC_SENS_VALUE);
        self.write_reg(syst, REG_VOLUME, 0xB000 | (VOL_GAIN << 4) | DAC_GAIN);

        // misc
        self.write_reg(syst, REG_PLL_VCO_BIAS, 0x5454);
        self.write_reg(syst, REG_FIXED_3E, 0xA037);
        self.write_reg(syst, REG_ANTI_SPUR, 0x88EF);
        self.write_reg(syst, REG_OOB_NOISE, 0x3732);
        self.write_reg(syst, REG_DISTORTION, 0x13A0);
    }
}
