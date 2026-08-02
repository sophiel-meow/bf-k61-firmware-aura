use crate::board;
use crate::hal::delay;
use cortex_m::peripheral::SYST;
use kd32f328_pac::gpiof;

const BIT_DELAY_US: u32 = 1;

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
const STATE_RX_ON: u16 = 0xBFF1; // 1 0 1111 1 11111 0001
const STATE_TX_ON: u16 = 0xC1FE; // 1 1 0000 0 11111 1110

const BANDWIDTH_WIDE: u16 = 0x3028;
const BANDWIDTH_NARROW: u16 = 0x4048;

const REG_SOFT_RESET: u8 = 0x00;
const REG_POWER: u8 = 0x37;
const REG_RXAGC_1: u8 = 0x13;
const REG_RXAGC_2: u8 = 0x12;
const REG_RXAGC_3: u8 = 0x11;
const REG_RXAGC_4: u8 = 0x10;
/// REG_RXAGC_5 (0x14) / REG_RXAGC_6 (0x49) are a matched pair: bottom step
/// of the RX AGC gain table plus the RF-AGC low/high threshold. A
/// "similiar chip" (BK4819) firmware writes a *different* pair
/// specifically while the active demod is AM, switched every time RX
/// modulation changes (`BK4819_InitAGC`/`RADIO_SetupAGC`).
/// `set_agc_modulation` below switches between them; values for
/// the AM pair are ported unmodified from that firmware.
const REG_RXAGC_5: u8 = 0x14;
const REG_RXAGC_6: u8 = 0x49;
const RXAGC_5_FM: u16 = 0x0019;
const RXAGC_6_FM: u16 = 0x2A38;
const RXAGC_5_AM: u16 = 0x0000;
const RXAGC_6_AM: u16 = 0x1920; // (50 << 7) | 32
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
/// REG 0x68: raw CTCSS frequency word as auto-decoded off air (bit15 = "not
/// found" flag, bits[12:0] the raw word -- same units as `subaudio_reg_word`
/// produces, convert with `ctcss_raw_to_tenths_hz`). Only valid while
/// `set_subaudio_scan_filter(true)` is active.
const REG_CTS_RAW: u8 = 0x68;
/// REG 0x69/0x6A: raw 23-bit DCS/CDCSS codeword as auto-decoded off air,
/// split as two 12-bit halves (0x69 bit15 = "not found" flag).
const REG_DCS_HI: u8 = 0x69;
const REG_DCS_LO: u8 = 0x6A;
/// REG 0x32: hardware wideband frequency-scan control (used by Search
/// mode's frequency-hunt). BIT0 enable; bits[15:14] scan-time (00=0.2s,
/// 01=0.4s, 10=0.8s, 11=1.6s per attempt). The vendor's own constant
/// (`REG_32 = 0x8244`) sets this to "10"/0.8s; since `Hunt` needs two
/// agreeing samples, that's up to 1.6s just to validate one reading. Set
/// to "00"/0.2s (the fastest option) instead -- a same-chip-family
/// (BK4819) firmware's equivalent frequency-scan feature uses exactly this
/// fastest setting, so it's a proven-safe choice on this hardware, not a
/// guess.
const REG_FREQ_SCAN_CTRL: u8 = 0x32;
const FREQ_SCAN_BASE: u16 = 0x0244;
/// REG 0x0D/0x0E: raw frequency-counter readback while the hardware
/// frequency-scan circuit is hunting. 0x0D bit15 is a "not ready yet" flag;
/// bits[10:0] are the counter's high bits, 0x0E holds the low 16 bits.
const REG_FREQ_SCAN_HI: u8 = 0x0D;
const REG_FREQ_SCAN_LO: u8 = 0x0E;
const REG_SUBAUDIO_THRESH: u8 = 0x52;
const SUBAUDIO_THRESH_VALUE: u16 = 0x0292;
/// REG 0x78: RSSI-based squelch threshold, 0.5dB/step.
/// [15:8] rssi_sq_th_in (squelch opens), [7:0] rssi_sq_th_out (squelch closes).
const REG_SQUELCH: u8 = 0x78;
const REG_STATUS: u8 = 0x0C;
const STATUS_SQ_OPEN: u16 = 0x0002;
const STATUS_SUBAUDIO_MATCH: u16 = 0x0400;
const STATUS_TAIL_DETECTED: u16 = 0x0800;
const STATUS_DCS_MATCH_NORMAL: u16 = 0x4000;
const STATUS_DCS_MATCH_INVERTED: u16 = 0x8000;

/// REG 0x08: the encoded 23-bit DCS/CDCSS bitstream, written as two 12-bit
/// halves (low half first, high half with bit15 set as a "this is the high
/// half" flag).
const REG_DCS_DATA: u8 = 0x08;
/// Standard DCS/CDCSS Golay(23,12) FEC generator constant
const DCS_GOLAY_POLY: u16 = 0x0C75;
/// `REG_SUBAUDIO_FREQ` value for the DCS/CDCSS bitstream's fixed 134.4 bit/s
/// clock: same frequency-word formula as a CTCSS tone (`tenths_hz *
/// 206489/100000`) evaluated at 134.4Hz, i.e. the digital-squelch baud rate
/// riding on the same register bank as the analog tone would.
const DCS_BAUD_WORD: u16 = 0x0AD7;

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
/// GPIO5 drives the red TX indicator LED (R_LED)
const GPIO5: u16 = 0x0020;

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

/// DTMF Goertzel-style coefficient table.
/// Same physical register (0x09) is
/// written 16 times; the coefficient's table index is packed into the top
/// 4 bits of each write and the coefficient itself into the low 12 bits.
const REG_DTMF_COEF: u8 = 0x09;
const DTMF_COEF_TABLE: [u16; 16] = [
    0x006F, 0x106B, 0x2067, 0x3062, 0x4050, 0x5047, 0x603A, 0x702C, 0x8041, 0x9037, 0xA025, 0xB017,
    0xC0E4, 0xD0CB, 0xE0B5, 0xF09F,
];

/// REG 0x72: FSK baud-rate select while in FSK work mode, or the DTMF
/// tone-2 frequency word while in DTMF work mode, same physical register,
/// meaning depends on which mode `REG_WORK_MODE` currently selects.
const REG_FSK_BAUD: u8 = 0x72;
const REG_FSK_5C: u8 = 0x5C;
const REG_FSK_5D: u8 = 0x5D;
const FSK_BAUD_1200: u16 = 0x3065; // <<1 for 2400
const FSK_5C_VALUE: u16 = 0x5665;
/// `(FSK_LEN * 2 - 1) << 8`, FSK_LEN = 8
const FSK_5D_VALUE: u16 = 0x0F00; // 0d15<<8 =0b0000_1111 << 8 = 0x0F00

/// REG 0x3F: baseband work-mode select (which special demod/mod block is
/// active). `REG_STATUS` (0x0C) bit0 is the shared "new data ready"
/// interrupt flag for whichever of these is currently selected.
const REG_WORK_MODE: u8 = 0x3F;
const WORK_MODE_DTMF: u16 = 0x0800;
const WORK_MODE_FSK_RX: u16 = 0x2000;
const WORK_MODE_FSK_TX_IRQ: u16 = 0x8000;
/// REG 0x02: write 0 to clear the pending work-mode interrupt flag.
const REG_INT_CLEAR: u8 = 0x02;

const REG_DTMF_ENABLE: u8 = 0x24;
const DTMF_ENABLE_BASE: u16 = 0x807F;
const DTMF_THRESHOLD: u16 = 130; // higher to ensure acc.
/// REG 0x0B[11:8]: last decoded DTMF digit nibble (keypad order: 0-9, then
/// A-D, then *, #, matching `DTMF_KEYPAD`'s index order).
const REG_DTMF_RESULT: u8 = 0x0B;
/// Fixed TX gain for the dual-tone oscillator while dialing DTMF (tone1
/// bits[14:8], tone2/FSK bits[6:0]).
const DTMF_TX_GAIN: u16 = 0xE0E0;

/// Standard telephone DTMF row/column tone pairs (Hz) for keypad digits in
/// `0..=9, A..=D, *, #` order -- the public DTMF standard, identical on
/// every phone and radio that supports it, not specific to this hardware.
const DTMF_KEYPAD: [(u16, u16); 16] = [
    (941, 1336), // 0
    (697, 1209), // 1
    (697, 1336), // 2
    (697, 1477), // 3
    (770, 1209), // 4
    (770, 1336), // 5
    (770, 1477), // 6
    (852, 1209), // 7
    (852, 1336), // 8
    (852, 1477), // 9
    (697, 1633), // A
    (770, 1633), // B
    (852, 1633), // C
    (941, 1633), // D
    (941, 1209), // *
    (941, 1477), // #
];

/// REG 0x58/0x59/0x5A-0x5D/0x5F: generic raw FSK modem (preamble, sync,
/// framing length, and the TX/RX FIFO). Shared substrate for any
/// byte-oriented FSK signaling; the DTMF/single-tone oscillator lives on
/// different registers entirely.
const REG_FSK_PREAMBLE: u8 = 0x58;
const REG_FSK_CONTROL: u8 = 0x59;
const REG_FSK_FIFO: u8 = 0x5F;
const FSK_CONTROL_BASE: u16 = 0x0028;
const FSK_CONTROL_RX_FIFO_CLEAR: u16 = 0x4000;
const FSK_CONTROL_TX_FIFO_CLEAR: u16 = 0x8000;
const FSK_CONTROL_RX_EN: u16 = 0x1000;
const FSK_CONTROL_TX_EN: u16 = 0x0800;
/// Raw FSK word count per frame (8 x 16-bit words = 16 bytes).
const FSK_FRAME_LEN: usize = 8;

/// MDC1200 roger tone
const MDC_PREAMBLE: u16 = 0x37C3;
const MDC_GAIN: u16 = 0x00B4;
/// REG 0x3F value while armed to receive the MDC1200 TX-complete ack.
const WORK_MODE_MDC_ARM: u16 = 0x3000;
const MDC_LEN: usize = 14;
/// `(MDC_LEN * 2 - 1) << 8`
const MDC_FRAME_LEN_FIELD: u16 = 0x1B00;
const REG_MDC_SYNC_HI: u8 = 0x5A;
const REG_MDC_SYNC_MID: u8 = 0x5B;
const MDC_SYNC: [u8; 5] = [0xFB, 0x72, 0x40, 0x99, 0xA7];
const MDC_TXDATA: [u16; MDC_LEN] = [
    0xB604, 0x37D9, 0x8388, 0xE28E, 0x66EC, 0xAEE4, 0xC3DB, 0x158F, 0x19A7, 0x499F, 0x82D4, 0xFAE5,
    0x1A96, 0xC9C0,
];

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
/// Extra REG 0x47 bit set while demodulating AM audio out (RX only).
const AM_AF_OUT_BIT: u16 = 0x0600;
/// REG_VOLUME is fixed to this value in AM mode instead of the calibrated
/// wideband/narrowband level.
const AM_VOLUME_FIXED: u16 = 0xB3AA;
/// Extra REG 0x47 bit set while demodulating in `Modulation::Usb` mode:
/// ORed onto `AF_STATE_RX_AUDIO` (AF-select field = 1, "FM") this walks the
/// 4-bit AF-select field from 1 to 5
const USB_AF_OUT_BIT: u16 = 0x0400;

const REG_UNKNOWN_3D: u8 = 0x3D;
const UNKNOWN_3D_NORMAL: u16 = 0x2AAB;
const UNKNOWN_3D_USB: u16 = 0x0000;

/// REG 0x73 bit4: AFC (automatic frequency control) disable
/// Needs to be on (AFC off) for anything but FM
const REG_AFC_DISABLE: u8 = 0x73;
const AFC_DISABLE_BIT: u16 = 0x0010;

/// REG 0x71: single-tone oscillator frequency (shared by the UI key-beep,
/// the 1750Hz repeater tone, and the roger beep).
/// REG 0x70: its gain/enable.
const REG_TONE_FREQ: u8 = 0x71;
const REG_TONE_GAIN: u8 = 0x70;
const TONE_GAIN_MASK: u16 = 0x70FF;
/// Fixed tone gain (60) plus the enable bit; only bits outside the mask
/// above are forced, the rest of whatever REG 0x70 already held survives.
const TONE_GAIN_ON_BITS: u16 = 0x8000 | (60 << 8);

/// REG_STATE values for the two tone-oscillator run modes:
/// `false`: sidetone only, transmitter left alone, what the UI key-beep uses
/// `true`: also keys the transmitter so the tone actually goes out over RF,
/// what the repeater-access tone and roger beep use
const STATE_TONE: u16 = 0x0302;
const STATE_TX_TONE: u16 = 0xC3FA;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AfOutState {
    Mute,
    RxAudio,
    #[allow(dead_code)]
    RxAlarmTone,
    Beep,
    #[allow(dead_code)]
    CtcDcsTest,
    #[allow(dead_code)]
    FskTest,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modulation {
    Fm,
    Am,
    Usb,
    /// CW: RX demodulates as USB (beat tone), TX is a bare keyed carrier.
    Cw,
    /// CW-over-FM: RX/TX both plain FM, sidetone is what actually
    /// modulates the carrier
    Cwf,
}

/// A tone auto-detected off air by the hardware decoder
/// before matching against a standard table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RawTone {
    None,
    /// Raw CTCSS frequency word, same units `subaudio_reg_word` produces.
    Ctcss(u16),
    /// Raw 23-bit DCS/CDCSS codeword.
    Dcs(u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SubAudio {
    None,
    /// Analog CTCSS tone, in tenths of Hz (e.g. `1000` = 100.0Hz).
    Ctcss(u16),
    /// Digital-coded squelch. `code` is the plain-binary DCS tone value
    /// (see `DCS_TABLE`, e.g. `0o023`); `inverted` selects the "I" (inverted
    /// polarity) vs "N" (normal) variant.
    Dcs {
        code: u16,
        inverted: bool,
    },
}

/// Which status-register bit means "sub-audio matched" for the currently
/// configured RX sub-audio kind, CTCSS and the two DCS polarities each use
/// a different bit of `REG_STATUS`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SubaudioMatchBit {
    Ctcss,
    DcsNormal,
    DcsInverted,
}

/// Voice-inversion scramble groups.
/// FIXME: the 4th value
const SCRAMBLE_TABLE: [u16; 3] = [0x8517, 0x770E, 0x7A90];
/// `REG_DEVIATION` value while a scramble group is active (vs
/// `DEVIATION_VALUE` normally).
const DEVIATION_SCRAMBLE_VALUE: u16 = 0x0450;

/// 26 MHz xtal calibration table
/// The index is derived from the XTAL_ADJUST byte in spi flash
/// Entry 8 corresponds to the nominal frequency (no correction)
const XTAL_ADJUST_TABLE: [u32; 17] = [
    40, 35, 30, 25, 20, 15, 10, 5, 0, 5, 10, 15, 20, 25, 30, 35, 40,
];
const XTAL_ADJUST_ZERO_POINT: u8 = 8;

const POWER_UP_VALUE: u16 = 0x1D0F;
/// RX-mode value for REG_POWER (0x37) — same base as `POWER_UP_VALUE` but
/// with BIT9 (0x200) set
const POWER_RX_VALUE: u16 = 0x1F0F;
/// REG_POWER (0x37) value for RX duty-cycle power-save: same base as
/// `POWER_UP_VALUE` but with the low nibble (xtal-osc enable + bandgap-ref
/// enable) cleared
const POWER_SLEEP_VALUE: u16 = POWER_UP_VALUE & 0xFFF0;
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
    dcs_mod_depth: u8,
    pa_target: u8,
    rx_match_bit: SubaudioMatchBit,
    tone_active: bool,
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
            dcs_mod_depth: 0,
            pa_target: 100,
            rx_match_bit: SubaudioMatchBit::Ctcss,
            tone_active: false,
        }
    }

    /// xtal calibration byte in flash (0xF210+6)
    pub fn set_xtal_adjust(&mut self, value: u8) {
        self.xtal_adjust = value;
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

    /// Releases both TX band-path GPIOs.
    ///
    /// Has to happen on the way back to RX: these stay latched at whatever
    /// `set_tx_band_*` left them, and leaving the transmit path asserted
    pub fn set_tx_band_off(&mut self, syst: &mut SYST) {
        self.set_gpio_bit(syst, GPIO2, false);
        self.set_gpio_bit(syst, GPIO3, false);
    }

    /// Red TX indicator LED
    pub fn set_tx_led(&mut self, syst: &mut SYST, on: bool) {
        self.set_gpio_bit(syst, GPIO5, on);
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
        // Modulation only changes the AfOutState::RxAudio path; irrelevant here.
        self.set_af_out(syst, AfOutState::Mute, true, Modulation::Fm);
        self.set_gpio_bit(syst, GPIO2, false);
        self.set_gpio_bit(syst, GPIO3, false);
    }

    /// Calibration bytes from the flash block at 0xF210 : offset 0 is mic
    /// modulation depth, offset 1 is CTCSS modulation depth, offset 2 is DCS
    /// modulation depth, offsets 3/4 are wideband/narrowband AF volume (max
    /// 31 each).
    pub fn set_audio_calibration(
        &mut self,
        mic_mod_depth: u8,
        cts_mod_depth: u8,
        dcs_mod_depth: u8,
        vol_wideband: u8,
        vol_narrowband: u8,
    ) {
        self.mic_mod_depth = mic_mod_depth;
        self.cts_mod_depth = cts_mod_depth;
        self.dcs_mod_depth = dcs_mod_depth;
        self.vol_wideband = vol_wideband;
        self.vol_narrowband = vol_narrowband;
    }

    /// Starts the single-tone oscillator at `hz_div_10` (frequency in units
    /// of 10Hz, e.g. `175` = 1750Hz) and routes it to the audio-out path.
    /// `key_tx = false` generates the tone purely on the local sidetone path,
    /// transmitter left alone (the UI key-beep); `key_tx = true` also keys the
    /// transmitter so the tone actually goes out over RF (the repeater-access
    /// tone and roger beep).
    pub fn tx_single_tone_on(&mut self, syst: &mut SYST, hz_div_10: u16, key_tx: bool) {
        let gain = self.read_reg(syst, REG_TONE_GAIN);
        self.write_reg(
            syst,
            REG_TONE_GAIN,
            (gain & TONE_GAIN_MASK) | TONE_GAIN_ON_BITS,
        );
        if !self.tone_active {
            self.write_reg(syst, REG_STATE, 0x0000);
        }
        self.write_reg(
            syst,
            REG_STATE,
            if key_tx { STATE_TX_TONE } else { STATE_TONE },
        );
        self.tone_active = true;
        // Beep volume is fixed regardless of modulation; irrelevant here.
        self.set_af_out(syst, AfOutState::Beep, true, Modulation::Fm);

        self.write_reg(
            syst,
            REG_TONE_FREQ,
            Self::tone_reg_word(hz_div_10 as u32 * 10),
        );
    }

    /// Stops the tone and falls back to plain RX or TX
    pub fn tx_single_tone_off(&mut self, syst: &mut SYST, was_transmitting: bool) {
        self.write_reg(syst, REG_TONE_FREQ, 0);
        self.write_reg(syst, REG_TONE_GAIN, 0x0000);
        self.tone_active = false;
        if was_transmitting {
            self.tx_on(syst);
        } else {
            self.rx_on(syst);
        }
    }

    pub fn tx_single_tone_off_idle(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_TONE_FREQ, 0);
        self.write_reg(syst, REG_TONE_GAIN, 0x0000);
        self.tone_active = false;
        self.idle(syst);
    }

    /// `REG_TONE_FREQ`/`REG_FSK_BAUD`'s frequency-word encoding when used as
    /// the single-tone oscillator or DTMF's dual-tone generator: `hz *
    /// 1032444 / 100000`, `hz` in whole Hz
    fn tone_reg_word(hz: u32) -> u16 {
        (hz * 1_032_444 / 100_000) as u16
    }

    /// `REG_SUBAUDIO_FREQ`'s frequency-word encoding for an analog CTCSS
    /// tone or the DCS/CDCSS bitstream's fixed baud clock: `tenths_hz *
    /// 206489 / 100000`.
    fn subaudio_reg_word(tenths_hz: u16) -> u16 {
        ((tenths_hz as u32 * 206_489) / 100_000) as u16
    }

    /// Standard DCS/CDCSS Golay(23,12) forward-error-correction encode.
    /// `code` is the plain-binary DCS tone value (bits 0-8ish, e.g. from
    /// `DCS_TABLE`); bit 11 is the format's fixed "always high" framing bit.
    /// Returns the 23-bit codeword the chip expects on `REG_DCS_DATA`.
    fn dcs_encode(code: u16) -> u32 {
        let source = code | 0x0800;
        let mut remainder: u16 = 0;
        let mut bit: u16 = 1;
        for _ in 0..12 {
            let source_bit = source & bit != 0;
            let remainder_bit = remainder & 0x0002 != 0;
            let feedback = if source_bit == remainder_bit {
                0
            } else {
                DCS_GOLAY_POLY
            };
            bit <<= 1;
            remainder >>= 1;
            remainder ^= feedback;
        }
        (source as u32) | ((remainder as u32) << 11)
    }

    fn write_dcs_codeword(&mut self, syst: &mut SYST, code: u16) {
        let word = Self::dcs_encode(code);
        self.write_reg(syst, REG_DCS_DATA, (word & 0x0FFF) as u16);
        self.write_reg(
            syst,
            REG_DCS_DATA,
            (((word >> 12) & 0x0FFF) as u16) | 0x8000,
        );
    }

    /// Selects a voice-inversion scramble group. `group == 0` turns
    /// scrambling off; `1..=3` selects a group
    pub fn set_scramble(&mut self, syst: &mut SYST, group: u8) {
        let scramble_reg = self.read_reg(syst, REG_SCRAMBLE);
        if group == 0 {
            self.write_reg(syst, REG_SCRAMBLE, scramble_reg & !0x0002);
            let dev = self.read_reg(syst, REG_DEVIATION) & 0xF000;
            self.write_reg(syst, REG_DEVIATION, dev | DEVIATION_VALUE);
        } else {
            let idx = (group.min(3) - 1) as usize;
            self.write_reg(syst, REG_SCRAMBLE, scramble_reg | 0x0002);
            self.write_reg(syst, REG_TONE_FREQ, SCRAMBLE_TABLE[idx]);
            let dev = self.read_reg(syst, REG_DEVIATION) & 0xF000;
            self.write_reg(syst, REG_DEVIATION, dev | DEVIATION_SCRAMBLE_VALUE);
        }
    }

    /// Enters DTMF work mode. `tx = true` also sets the fixed encode gain,
    /// for dialing digits with `set_dtmf_digit`; `tx = false` is for
    /// decode-only (RX) use with `dtmf_digit_ready`/`read_dtmf_digit`.
    pub fn enter_dtmf_mode(&mut self, syst: &mut SYST, tx: bool) {
        self.write_reg(
            syst,
            REG_DTMF_ENABLE,
            DTMF_ENABLE_BASE | (DTMF_THRESHOLD << 7),
        );
        if tx {
            self.write_reg(syst, REG_TONE_GAIN, DTMF_TX_GAIN);
        }
        self.write_reg(syst, REG_WORK_MODE, WORK_MODE_DTMF);
    }

    pub fn exit_dtmf_mode(&mut self, syst: &mut SYST) {
        let temp = self.read_reg(syst, REG_DTMF_ENABLE) & 0xFFDF;
        self.write_reg(syst, REG_DTMF_ENABLE, temp);
        self.write_reg(syst, REG_TONE_GAIN, 0x0000);
    }

    /// Drives (or silences, with `None`) the DTMF dual-tone oscillator for
    /// one keypad digit
    pub fn set_dtmf_digit(&mut self, syst: &mut SYST, digit: Option<u8>) {
        let (tone1, tone2) = match digit {
            Some(d) => DTMF_KEYPAD[(d & 0x0F) as usize],
            None => (0, 0),
        };
        let word1 = if tone1 == 0 {
            0
        } else {
            Self::tone_reg_word(tone1 as u32)
        };
        let word2 = if tone2 == 0 {
            0
        } else {
            Self::tone_reg_word(tone2 as u32)
        };
        self.write_reg(syst, REG_TONE_FREQ, word1);
        self.write_reg(syst, REG_FSK_BAUD, word2);
    }

    /// True once a new decoded DTMF digit is ready (RX use)
    pub fn dtmf_digit_ready(&mut self, syst: &mut SYST) -> bool {
        if self.read_reg(syst, REG_WORK_MODE) & WORK_MODE_DTMF == 0 {
            return false;
        }
        if self.read_reg(syst, REG_STATUS) & 0x0001 != 0 {
            self.write_reg(syst, REG_INT_CLEAR, 0x0000);
            true
        } else {
            false
        }
    }

    /// Last decoded DTMF digit nibble, in `DTMF_KEYPAD` order.
    pub fn read_dtmf_digit(&mut self, syst: &mut SYST) -> u8 {
        ((self.read_reg(syst, REG_DTMF_RESULT) >> 8) & 0x0F) as u8
    }

    /// Enables the raw FSK modem for byte-oriented framed TX/RX (the
    /// generic substrate; not a protocol encoder by itself)
    pub fn enter_fsk_mode(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_FSK_PREAMBLE, 0x00C1);
        self.write_reg(syst, REG_TONE_GAIN, 0x00E0);
        self.write_reg(syst, REG_FSK_BAUD, FSK_BAUD_1200);
        self.write_reg(syst, REG_FSK_5C, FSK_5C_VALUE);
        self.write_reg(syst, REG_FSK_5D, FSK_5D_VALUE);
        self.write_reg(
            syst,
            REG_FSK_CONTROL,
            FSK_CONTROL_BASE | FSK_CONTROL_RX_FIFO_CLEAR,
        );
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE | FSK_CONTROL_RX_EN);
        self.write_reg(syst, REG_WORK_MODE, WORK_MODE_FSK_RX);
    }

    pub fn exit_fsk_mode(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_TONE_GAIN, 0x0000);
        self.write_reg(syst, REG_FSK_PREAMBLE, 0x0000);
    }

    /// Transmits one raw `FSK_FRAME_LEN`-word (16-byte) frame through the
    /// modem's FIFO and blocks until the chip reports it sent (or ~1s
    /// elapses without an ack)
    pub fn fsk_transmit(&mut self, syst: &mut SYST, data: &[u16; FSK_FRAME_LEN]) {
        self.write_reg(syst, REG_WORK_MODE, WORK_MODE_FSK_TX_IRQ);
        self.write_reg(
            syst,
            REG_FSK_CONTROL,
            FSK_CONTROL_BASE | FSK_CONTROL_TX_FIFO_CLEAR,
        );
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE);
        for &word in data.iter() {
            self.write_reg(syst, REG_FSK_FIFO, word);
        }
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE | FSK_CONTROL_TX_EN);
        for _ in 0..200 {
            delay::ms(syst, 5);
            if self.read_reg(syst, REG_STATUS) & 0x0001 != 0 {
                break;
            }
        }
        self.write_reg(syst, REG_INT_CLEAR, 0x0000);
        self.write_reg(syst, REG_WORK_MODE, 0x0000);
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE);
    }

    /// True once a full raw FSK frame is ready to read.
    pub fn fsk_rx_ready(&mut self, syst: &mut SYST) -> bool {
        self.read_reg(syst, REG_WORK_MODE) & WORK_MODE_FSK_RX != 0
            && self.read_reg(syst, REG_STATUS) & 0x0001 != 0
    }

    pub fn fsk_receive(&mut self, syst: &mut SYST) -> [u16; FSK_FRAME_LEN] {
        let mut data = [0u16; FSK_FRAME_LEN];
        for slot in data.iter_mut() {
            *slot = self.read_reg(syst, REG_FSK_FIFO);
        }
        self.write_reg(syst, REG_INT_CLEAR, 0x0000);
        data
    }

    pub fn enter_mdc1200_mode(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_FSK_PREAMBLE, MDC_PREAMBLE);
        self.write_reg(syst, REG_FSK_BAUD, FSK_BAUD_1200);
        self.write_reg(syst, REG_TONE_GAIN, MDC_GAIN);
        self.write_reg(syst, REG_FSK_5D, MDC_FRAME_LEN_FIELD);
        self.write_reg(
            syst,
            REG_FSK_CONTROL,
            FSK_CONTROL_BASE | FSK_CONTROL_RX_FIFO_CLEAR,
        );
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE | FSK_CONTROL_RX_EN);
        self.write_reg(syst, REG_WORK_MODE, WORK_MODE_MDC_ARM);
    }

    pub fn exit_mdc1200_mode(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_TONE_GAIN, 0x0000);
        self.write_reg(syst, REG_FSK_PREAMBLE, 0x0000);
    }

    /// Sends the fixed MDC1200 end-of-transmission burst and blocks until
    /// the chip reports it sent (or ~1s elapses without an ack).
    pub fn mdc1200_tone_tx(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_WORK_MODE, WORK_MODE_FSK_TX_IRQ);
        self.write_reg(
            syst,
            REG_FSK_CONTROL,
            FSK_CONTROL_BASE | FSK_CONTROL_TX_FIFO_CLEAR,
        );
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE);

        let sync_hi = (MDC_SYNC[0] as u16) << 8 | MDC_SYNC[1] as u16;
        let sync_mid = (MDC_SYNC[2] as u16) << 8 | MDC_SYNC[3] as u16;
        let sync_lo = (MDC_SYNC[4] as u16) << 8 | 0x30;
        self.write_reg(syst, REG_MDC_SYNC_HI, sync_hi);
        self.write_reg(syst, REG_MDC_SYNC_MID, sync_mid);
        self.write_reg(syst, REG_FSK_5C, sync_lo);

        for &word in MDC_TXDATA.iter() {
            self.write_reg(syst, REG_FSK_FIFO, word);
        }
        delay::ms(syst, 20);

        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE | FSK_CONTROL_TX_EN);
        for _ in 0..200 {
            delay::ms(syst, 5);
            if self.read_reg(syst, REG_STATUS) & 0x0001 != 0 {
                break;
            }
        }
        self.write_reg(syst, REG_INT_CLEAR, 0x0000);
        self.write_reg(syst, REG_WORK_MODE, 0x0000);
        self.write_reg(syst, REG_FSK_CONTROL, FSK_CONTROL_BASE);
    }

    /// Selects what REG 0x47 routes to the audio-out pin and switches
    /// REG_VOLUME (0x48) alongside it. `wide` selects which calibrated
    /// volume level applies when the state isn't a fixed-volume tone.
    /// In `Modulation::Am`, RX audio gets an extra REG 0x47 bit and a fixed
    /// REG_VOLUME value instead of the wideband/narrowband calibration.
    pub fn set_af_out(
        &mut self,
        syst: &mut SYST,
        state: AfOutState,
        wide: bool,
        modulation: Modulation,
    ) {
        let state_bits = match state {
            AfOutState::Mute => AF_STATE_MUTE,
            AfOutState::RxAudio => AF_STATE_RX_AUDIO,
            AfOutState::RxAlarmTone => AF_STATE_RX_ALARM_TONE,
            AfOutState::Beep => AF_STATE_BEEP,
            AfOutState::CtcDcsTest => AF_STATE_CTC_DCS_TEST,
            AfOutState::FskTest => AF_STATE_FSK_TEST,
        };
        let mut af_out = AF_OUT_BASE | state_bits;
        if state == AfOutState::RxAudio {
            match modulation {
                Modulation::Am => af_out |= AM_AF_OUT_BIT,
                Modulation::Usb | Modulation::Cw => af_out |= USB_AF_OUT_BIT,
                Modulation::Fm | Modulation::Cwf => {}
            }
        }
        self.write_reg(syst, REG_AF_OUT, af_out);

        let vol_reg = if state == AfOutState::Beep {
            AF_OUT_BEEP_VOLUME
        } else if modulation == Modulation::Am {
            AM_VOLUME_FIXED
        } else {
            let vol = (if wide {
                self.vol_wideband
            } else {
                self.vol_narrowband
            } & 0x3F) as u16;
            0x8000 | (vol << 4) | DAC_GAIN
        };
        self.write_reg(syst, REG_VOLUME, vol_reg);
    }

    pub fn apply_modulation(&mut self, syst: &mut SYST, modulation: Modulation) {
        let unknown_3d = if matches!(modulation, Modulation::Usb | Modulation::Cw) {
            UNKNOWN_3D_USB
        } else {
            UNKNOWN_3D_NORMAL
        };
        self.write_reg(syst, REG_UNKNOWN_3D, unknown_3d);

        let afc_disable = self.read_reg(syst, REG_AFC_DISABLE);
        let afc_disable = if !matches!(modulation, Modulation::Fm | Modulation::Cwf) {
            afc_disable | AFC_DISABLE_BIT
        } else {
            afc_disable & !AFC_DISABLE_BIT
        };
        self.write_reg(syst, REG_AFC_DISABLE, afc_disable);

        let (agc5, agc6) = if modulation == Modulation::Am {
            (RXAGC_5_AM, RXAGC_6_AM)
        } else {
            (RXAGC_5_FM, RXAGC_6_FM)
        };
        self.write_reg(syst, REG_RXAGC_5, agc5);
        self.write_reg(syst, REG_RXAGC_6, agc6);
    }

    pub fn apply_tx_mic_gain(&mut self, syst: &mut SYST) {
        let gain = (self.mic_mod_depth % 32) as u16;
        self.write_reg(syst, REG_MIC_SENS, (MIC_SENS_VALUE & 0xFFE0) | gain);
    }

    /// Configures the transmitted sub-audible tone/code.
    ///
    /// `REG_CTCSS` (0x51): bit15 `subau_en`, bit13 `dcs_polarity` (1=
    /// inverted), bit12 `ctc_dcs_sel` (1=CTCSS, 0=DCS), bits[6:0]
    /// `subau_gain`.
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
                let word = Self::subaudio_reg_word(tenths_hz);
                self.write_reg(syst, REG_SUBAUDIO_FREQ, word & 0x1FFF);

                self.write_reg(syst, REG_SUBAUDIO_FREQ, SUBAUDIO_TAIL_WORD);
                self.write_reg(syst, REG_SUBAUDIO_THRESH, SUBAUDIO_THRESH_VALUE);
            }
            SubAudio::Dcs { code, inverted } => {
                let gain = (self.dcs_mod_depth & 0x7F) as u16;
                let mask = if inverted { 0xA000 } else { 0x8000 };
                self.write_reg(syst, REG_CTCSS, mask | gain);
                self.write_reg(syst, REG_SUBAUDIO_FREQ, DCS_BAUD_WORD);
                self.write_dcs_codeword(syst, code);
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
        match sub {
            SubAudio::Dcs { code, inverted } => {
                self.rx_match_bit = if inverted {
                    SubaudioMatchBit::DcsInverted
                } else {
                    SubaudioMatchBit::DcsNormal
                };
                let gain = (self.dcs_mod_depth & 0x7F) as u16;
                let mask = if inverted { 0xA000 } else { 0x8000 };
                self.write_reg(syst, REG_CTCSS, mask | gain);
                self.write_reg(syst, REG_SUBAUDIO_FREQ, DCS_BAUD_WORD);
                self.write_dcs_codeword(syst, code);
            }
            _ => {
                self.rx_match_bit = SubaudioMatchBit::Ctcss;
                let primary_tenths_hz = match sub {
                    SubAudio::Ctcss(tenths_hz) => tenths_hz,
                    _ => 551, // 55.1Hz — same as the fixed tail tone
                };
                let gain = (self.cts_mod_depth & 0x7F) as u16;
                self.write_reg(syst, REG_CTCSS, 0x9000 | gain);
                let primary_word = Self::subaudio_reg_word(primary_tenths_hz);
                self.write_reg(syst, REG_SUBAUDIO_FREQ, primary_word & 0x1FFF);
                self.write_reg(syst, REG_SUBAUDIO_FREQ, SUBAUDIO_TAIL_WORD);
                self.write_reg(syst, REG_SUBAUDIO_THRESH, SUBAUDIO_THRESH_VALUE);
            }
        }
    }

    pub fn tail_detected(&mut self, syst: &mut SYST) -> bool {
        self.read_reg(syst, REG_STATUS) & STATUS_TAIL_DETECTED != 0
    }

    pub fn subaudio_matched(&mut self, syst: &mut SYST) -> bool {
        let status = self.read_reg(syst, REG_STATUS);
        let bit = match self.rx_match_bit {
            SubaudioMatchBit::Ctcss => STATUS_SUBAUDIO_MATCH,
            SubaudioMatchBit::DcsNormal => STATUS_DCS_MATCH_NORMAL,
            SubaudioMatchBit::DcsInverted => STATUS_DCS_MATCH_INVERTED,
        };
        status & bit != 0
    }

    /// Enables/disables the hardware wideband frequency-scan circuit used
    /// by Search mode's frequency-hunt.
    pub fn freq_scan_enable(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_FREQ_SCAN_CTRL, FREQ_SCAN_BASE | 0x0001);
    }

    pub fn freq_scan_disable(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_FREQ_SCAN_CTRL, FREQ_SCAN_BASE);
    }

    /// Raw frequency-counter reading while the hardware frequency-scan
    /// circuit is hunting; `None` while it hasn't locked onto a candidate
    /// yet.
    pub fn check_freq_scan(&mut self, syst: &mut SYST) -> Option<u32> {
        let hi = self.read_reg(syst, REG_FREQ_SCAN_HI);
        if hi & 0x8000 != 0 {
            return None;
        }
        let lo = self.read_reg(syst, REG_FREQ_SCAN_LO);
        Some((((hi & 0x07FF) as u32) << 16) | lo as u32)
    }

    /// Toggles the wide subaudio-filter bandwidth used while auto-detecting
    /// an unknown CTCSS/DCS tone, as opposed to decoding a specific
    /// configured one. Shares REG_CTCSS (0x51) with `set_subaudio_tx`/
    /// `enable_rx_subaudio` -- callers must re-apply the normal subaudio
    /// config after turning this back off. Ported from
    /// `Rfic_CtcDcsScan_Setup`/`Rfic_ScanQT_Enable`/`Rfic_ScanQT_Disable`.
    pub fn set_subaudio_scan_filter(&mut self, syst: &mut SYST, enabled: bool) {
        self.write_reg(syst, REG_CTCSS, if enabled { 0x0300 } else { 0x0000 });
    }

    /// Reads the hardware's auto-decoded CTCSS/DCS tone, if any is
    /// currently locked. Requires `set_subaudio_scan_filter(true)` to have
    /// been applied first. Ported from `Rfic_GetCtsDcsData`; the original's
    /// stuck-bit noise reject re-checked byte 0 five times instead of each
    /// of the 5 sampled bytes (an apparent copy-paste bug) -- this checks
    /// all 5 as clearly intended.
    pub fn detect_subaudio_raw(&mut self, syst: &mut SYST) -> RawTone {
        let dcs_hi = self.read_reg(syst, REG_DCS_HI);
        if dcs_hi & 0x8000 == 0 {
            let dcs_lo = self.read_reg(syst, REG_DCS_LO);
            let raw = (((dcs_hi & 0x0FFF) as u32) << 12) | (dcs_lo & 0x0FFF) as u32;
            let bytes = [
                (raw & 0xFF) as u8,
                ((raw >> 4) & 0xFF) as u8,
                ((raw >> 8) & 0xFF) as u8,
                ((raw >> 12) & 0xFF) as u8,
                ((raw >> 16) & 0xFF) as u8,
            ];
            let stuck_ff = bytes.iter().filter(|&&b| b == 0xFF).count() >= 4;
            let stuck_00 = bytes.iter().filter(|&&b| b == 0x00).count() >= 4;
            return if stuck_ff || stuck_00 {
                RawTone::None
            } else {
                RawTone::Dcs(raw)
            };
        }

        let cts = self.read_reg(syst, REG_CTS_RAW);
        if cts & 0x8000 == 0 {
            return RawTone::Ctcss(cts & 0x1FFF);
        }

        RawTone::None
    }

    /// Reverses `subaudio_reg_word`: raw register word -> tenths of Hz.
    /// Ported from the conversion done inline in `SearchFreqTask`.
    pub fn ctcss_raw_to_tenths_hz(raw: u16) -> u16 {
        ((raw as u32 * 100_000) / 206_489) as u16
    }

    /// Matches a detected CTCSS tone (tenths of Hz) against a standard
    /// table, +/-0.9Hz tolerance. Ported from `CheckIsStandardCTCSS`.
    pub fn find_standard_ctcss(tenths_hz: u16, table: &[u16]) -> Option<u16> {
        table.iter().copied().find(|&t| tenths_hz.abs_diff(t) < 10)
    }

    /// Matches a detected 23-bit DCS/CDCSS codeword against a standard
    /// table by re-encoding each candidate and comparing at all 23
    /// bit-rotations (the receiver can lock onto any phase of the
    /// repeating bitstream). Only checks normal polarity -- same
    /// limitation as the original `CheckIsStandardDCS`, which never
    /// reports an inverted-polarity match. Returns the plain DCS code.
    pub fn find_standard_dcs(raw23: u32, table: &[u16]) -> Option<u16> {
        let raw23 = raw23 & 0x007F_FFFF;
        for &code in table {
            let mut word = Self::dcs_encode(code) & 0x007F_FFFF;
            for _ in 0..23 {
                word = ((word << 1) | (word >> 22)) & 0x007F_FFFF;
                if word == raw23 {
                    return Some(code);
                }
            }
        }
        None
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

    pub fn sleep(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, 0x0000);
        self.write_reg(syst, REG_POWER, POWER_SLEEP_VALUE);
    }

    /// Sets REG_POWER (0x37) to the RX-specific value (`0x1F0F`, with BIT9).
    pub fn power_rx(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_POWER, POWER_RX_VALUE);
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

    /// Inverse of the xtal correction `set_frequency_hz` applies when
    /// tuning: converts a *measured* raw frequency-counter word (deci-Hz,
    /// back into the true frequency word, for Search mode's frequency-hunt.
    pub fn correct_measured_freq_word(&self, raw_word: u32) -> u32 {
        let idx = if self.xtal_adjust > 16 {
            XTAL_ADJUST_ZERO_POINT
        } else {
            self.xtal_adjust
        } as usize;
        let adjust = raw_word * XTAL_ADJUST_TABLE[idx] / 10_000_000;
        if idx as u8 > XTAL_ADJUST_ZERO_POINT {
            raw_word - adjust
        } else {
            raw_word + adjust
        }
    }

    /// REG 0x67[8:1]：RSSI
    pub fn get_rssi(&mut self, syst: &mut SYST) -> u16 {
        (self.read_reg(syst, REG_RSSI) & 0x01FF) >> 1
    }

    pub fn set_wide_bandwidth(&mut self, syst: &mut SYST, wide: bool) {
        let value = if wide {
            BANDWIDTH_WIDE
        } else {
            BANDWIDTH_NARROW
        };
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
        self.write_reg(syst, REG_RXAGC_5, RXAGC_5_FM);
        self.write_reg(syst, REG_RXAGC_6, RXAGC_6_FM);
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
