//! Bit-banged I2C master

use crate::board;
use crate::hal::delay;
use cortex_m::peripheral::SYST;
use kd32f328_pac::gpioa;

const BIT_DELAY_US: u32 = 10;

pub struct I2cBus<'a> {
    gpioa: &'a gpioa::RegisterBlock,
}

impl<'a> I2cBus<'a> {
    pub fn new(gpioa: &'a gpioa::RegisterBlock) -> Self {
        I2cBus { gpioa }
    }

    pub fn start(&mut self, syst: &mut SYST) {
        board::set_i2c_sda(self.gpioa, true);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_scl(self.gpioa, true);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_sda(self.gpioa, false);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_scl(self.gpioa, false);
        delay::us(syst, BIT_DELAY_US);
    }

    pub fn stop(&mut self, syst: &mut SYST) {
        board::set_i2c_scl(self.gpioa, false);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_sda(self.gpioa, false);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_scl(self.gpioa, true);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_sda(self.gpioa, true);
        delay::us(syst, BIT_DELAY_US);
    }

    pub fn write_byte(&mut self, syst: &mut SYST, mut byte: u8) -> bool {
        for _ in 0..8 {
            board::set_i2c_sda(self.gpioa, byte & 0x80 != 0);
            byte <<= 1;
            delay::us(syst, BIT_DELAY_US);
            board::set_i2c_scl(self.gpioa, true);
            delay::us(syst, BIT_DELAY_US);
            board::set_i2c_scl(self.gpioa, false);
            delay::us(syst, BIT_DELAY_US);
        }

        board::set_i2c_sda_input(self.gpioa);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_scl(self.gpioa, true);
        delay::us(syst, BIT_DELAY_US);
        let ack = !board::read_i2c_sda(self.gpioa);
        board::set_i2c_scl(self.gpioa, false);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_sda_output(self.gpioa);
        ack
    }

    pub fn read_byte(&mut self, syst: &mut SYST, ack: bool) -> u8 {
        let mut data = 0u8;
        board::set_i2c_sda_input(self.gpioa);
        delay::us(syst, BIT_DELAY_US);
        for _ in 0..8 {
            board::set_i2c_scl(self.gpioa, true);
            delay::us(syst, BIT_DELAY_US);
            data <<= 1;
            if board::read_i2c_sda(self.gpioa) {
                data |= 1;
            }
            board::set_i2c_scl(self.gpioa, false);
            delay::us(syst, BIT_DELAY_US);
        }

        board::set_i2c_sda_output(self.gpioa);
        board::set_i2c_sda(self.gpioa, !ack);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_scl(self.gpioa, true);
        delay::us(syst, BIT_DELAY_US);
        board::set_i2c_scl(self.gpioa, false);
        delay::us(syst, BIT_DELAY_US);
        data
    }
}
