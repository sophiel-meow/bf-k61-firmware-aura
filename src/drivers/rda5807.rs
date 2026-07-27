use crate::hal::i2c::I2cBus;
use cortex_m::peripheral::SYST;
use kd32f328_pac::gpioa;

const ADDR_WRITE: u8 = 0x20; // 0b0010000 + W
const ADDR_READ: u8 = 0x21; // 0b0010000 + R

/// REG02H: master control word.
///
/// | bit  | field                                                      |
/// |------|------------------------------------------------------------|
/// | 15   | DHIZ: 0 = audio output high-Z, 1 = normal                 |
/// | 14   | DMUTE: 0 = mute, 1 = normal                               |
/// | 13   | MONO: 0 = stereo, 1 = force mono                          |
/// | 12   | BASS: bass boost enable                                   |
/// | 9    | SEEKUP: 0 = seek down, 1 = seek up                        |
/// | 8    | SEEK: 0 = idle, 1 = start seeking                         |
/// | 7    | SKMODE: 0 = wrap at band edge, 1 = stop at band edge      |
/// | 6:4  | CLK_MODE: 110 = 26MHz. Not the chip's own default         |
/// |      | (32.768kHz crystal on RCLK): this board feeds RCLK from    |
/// |      | the 26MHz reference shared with the FD6818                 |
/// | 2    | NEW_METHOD: improves sensitivity ~1dB                       |
/// | 0    | ENABLE: chip power-up enable                                |
///
/// Bits not listed (RDS_EN, RCLK_NON_CALIBRATE, RCLK_DIRECT_INPUT,
/// SOFT_RESET) are left at their reset default of 0.
const CTRL_NORMAL: u16 = 0xD265;
const CTRL_SEEK: u16 = CTRL_NORMAL | 0x0100; // + SEEK=1
/// DMUTE=0 (mute) and ENABLE=0, everything else unchanged.
const CTRL_OFF: u16 = CTRL_NORMAL & !0x4001u16;

/// REG04H: DE=0 (75us de-emphasis), RDS FIFO cleared, AFC left enabled
/// (AFCD=0). RDS/I2S/GPIO all unused (0).
const CONFIG_REG04: u16 = 0x0400;
/// REG05H: SEEKTH/LNA fields left at datasheet reset defaults (SEEKTH=8,
/// LNA_PORT_SEL=LNAP, LNA_ICSEL=1.8mA). VOLUME forced to max (1111,
/// datasheet default is 1011), actual loudness is controlled elsewhere
const CONFIG_REG05: u16 = 0x888F;

/// REG03H BAND[1:0]. Only these two of the datasheet's four options are
/// used — the other two (87-108MHz US/Europe, 76-91MHz Japan) aren't
/// needed for this radio's supported tuning range.
/// TODO: expand freq range
const BAND_WORLDWIDE: u16 = 0b10; // 76-108MHz
/// 65-76MHz, conditional on REG07H bit9 (65M_50M_MODE), left at its
/// reset default of 1 (this driver never writes REG07H).
const BAND_LOW: u16 = 0b11;
const SPACE_100KHZ: u16 = 0b00;

const WORLDWIDE_BASE_KHZ: u32 = 76_000;
const LOW_BASE_KHZ: u32 = 65_000;
const CHANNEL_SPACING_KHZ: u32 = 100;

const STATUS_STC: u16 = 0x4000; // REG0AH bit14: tune/seek complete
const STATUS_SF: u16 = 0x2000; // REG0AH bit13: seek failed
const STATUS_FM_TRUE: u16 = 0x0100; // REG0BH bit8: current channel is a station

pub struct Rda5807<'a> {
    i2c: I2cBus<'a>,
}

impl<'a> Rda5807<'a> {
    pub fn new(gpioa: &'a gpioa::RegisterBlock) -> Self {
        Rda5807 { i2c: I2cBus::new(gpioa) }
    }

    /// Sequential write starting at REG02H.
    /// `regs[0]` -> REG02H
    /// `regs[1]` -> REG03H
    /// ...
    fn write_regs(&mut self, syst: &mut SYST, regs: &[u16]) {
        self.i2c.start(syst);
        self.i2c.write_byte(syst, ADDR_WRITE);
        for &value in regs {
            self.i2c.write_byte(syst, (value >> 8) as u8);
            self.i2c.write_byte(syst, value as u8);
        }
        self.i2c.stop(syst);
    }

    /// Sequential read starting at REG0AH. Returns (REG0AH, REG0BH).
    fn read_status(&mut self, syst: &mut SYST) -> (u16, u16) {
        self.i2c.start(syst);
        self.i2c.write_byte(syst, ADDR_READ);
        let a_hi = self.i2c.read_byte(syst, true);
        let a_lo = self.i2c.read_byte(syst, true);
        let b_hi = self.i2c.read_byte(syst, true);
        let b_lo = self.i2c.read_byte(syst, false);
        self.i2c.stop(syst);
        (((a_hi as u16) << 8) | a_lo as u16, ((b_hi as u16) << 8) | b_lo as u16)
    }

    pub fn set_frequency_khz(&mut self, syst: &mut SYST, freq_khz: u32) {
        let (base, band) = if freq_khz >= WORLDWIDE_BASE_KHZ {
            (WORLDWIDE_BASE_KHZ, BAND_WORLDWIDE)
        } else {
            (LOW_BASE_KHZ, BAND_LOW)
        };
        let chan = freq_khz.saturating_sub(base) / CHANNEL_SPACING_KHZ;
        let reg03 = ((chan as u16) << 6) | (1 << 4) /* TUNE */ | (band << 2) | SPACE_100KHZ;
        self.write_regs(syst, &[CTRL_NORMAL, reg03, CONFIG_REG04, CONFIG_REG05]);
    }

    pub fn seek(&mut self, syst: &mut SYST) {
        self.write_regs(syst, &[CTRL_SEEK]);
    }

    pub fn power_off(&mut self, syst: &mut SYST) {
        self.write_regs(syst, &[CTRL_OFF]);
    }

    /// `(tune_or_seek_complete, seek_failed, is_station, rssi)`. RSSI is
    /// REG0BH[15:9], 0..127 logarithmic.
    pub fn status(&mut self, syst: &mut SYST) -> (bool, bool, bool, u8) {
        let (a, b) = self.read_status(syst);
        let complete = a & STATUS_STC != 0;
        let seek_failed = a & STATUS_SF != 0;
        let is_station = b & STATUS_FM_TRUE != 0;
        let rssi = (b >> 9) as u8;
        (complete, seek_failed, is_station, rssi)
    }
}
