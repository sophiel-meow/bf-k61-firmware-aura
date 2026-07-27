use crate::board;
use crate::delay;
use cortex_m::peripheral::SYST;
use kd32f328_pac::gpiof;

const BIT_DELAY_US: u32 = 5;

const REG_FREQ_LO: u8 = 0x38;
const REG_FREQ_HI: u8 = 0x39;
const REG_STATE: u8 = 0x30;
const REG_BANDWIDTH: u8 = 0x43;

const STATE_IDLE: u16 = 0x0000;
const STATE_RX_ON: u16 = 0xBFF1;
const STATE_TX_ON: u16 = 0xC1FE;

const BANDWIDTH_WIDE: u16 = 0x3028;
const BANDWIDTH_NARROW: u16 = 0x4048;

pub struct Fd6818<'a> {
    gpiob: &'a gpiof::RegisterBlock,
}

impl<'a> Fd6818<'a> {
    pub fn new(gpiob: &'a gpiof::RegisterBlock) -> Self {
        Fd6818 { gpiob }
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

    /// set freq (Hz, min step 10Hz)
    pub fn set_frequency_hz(&mut self, syst: &mut SYST, freq_hz: u32) {
        let word = freq_hz / 10;
        self.write_reg(syst, REG_FREQ_LO, word as u16);
        self.write_reg(syst, REG_FREQ_HI, (word >> 16) as u16);
    }

    pub fn set_wide_bandwidth(&mut self, syst: &mut SYST, wide: bool) {
        let value = if wide { BANDWIDTH_WIDE } else { BANDWIDTH_NARROW };
        self.write_reg(syst, REG_BANDWIDTH, value);
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
}
