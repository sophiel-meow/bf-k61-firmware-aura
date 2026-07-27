use crate::board;
use crate::delay;
use cortex_m::peripheral::SYST;
use kd32f328_pac::gpiof;

const BIT_DELAY_US: u32 = 5;

const REG_FREQ_LO: u8 = 0x38;
const REG_FREQ_HI: u8 = 0x39;
const REG_STATE: u8 = 0x30;
const REG_BANDWIDTH: u8 = 0x43;
const REG_TX_POWER: u8 = 0x28;

const STATE_IDLE: u16 = 0x0000;
const STATE_RX_ON: u16 = 0xBFF1;
const STATE_TX_ON: u16 = 0xC1FE;

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
const REG_MIC_AGC: u8 = 0x19;
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
}

impl<'a> Fd6818<'a> {
    pub fn new(gpiob: &'a gpiof::RegisterBlock) -> Self {
        Fd6818 {
            gpiob,
            xtal_adjust: XTAL_ADJUST_ZERO_POINT,
        }
    }

    /// xtal calibration byte in flash (0xF210+6)
    pub fn set_xtal_adjust(&mut self, value: u8) {
        self.xtal_adjust = value;
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

    /// padrv_gain[2:0]=REG_28H[5:3], pa_gain_vreg[2:0]=REG_28H[2:0]，
    pub fn set_tx_power_min(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_TX_POWER, 0x0000); // TODO: minimal power
    }

    pub fn rx_on(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, STATE_RX_ON);
    }

    pub fn tx_on(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, STATE_TX_ON);
    }

    pub fn idle(&mut self, syst: &mut SYST) {
        self.write_reg(syst, REG_STATE, STATE_IDLE);
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

        // MIC AGC
        self.write_reg(syst, REG_MIC_AGC, 0x1041);

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
