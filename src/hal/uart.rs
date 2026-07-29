use kd32f328_pac::usart1;

pub struct DebugUart<'a> {
    regs: &'a usart1::RegisterBlock,
}

impl<'a> DebugUart<'a> {
    /// pclk_hz: bus freq (APB2 for USART1)
    /// OVER8 == 0 (16x oversampling)
    pub fn new(regs: &'a usart1::RegisterBlock, pclk_hz: u32, baud: u32) -> Self {
        // RAW BRR = round(pclk*16/baud)；
        // high 12 bit (bit4..15): mantissa
        // low 4 bit: fraction (1/16)
        let raw = (pclk_hz as u64 * 16 + baud as u64 / 2) / baud as u64;
        let mantissa = (raw >> 4) as u16;
        let fraction = (raw & 0xF) as u8;

        regs.brr().write(|w| unsafe {
            w.div_mantissa().bits(mantissa);
            w.div_fraction().bits(fraction)
        });

        regs.cr1().write(|w| {
            w.te().set_bit(); // only tx
            w.ue().set_bit()
        });

        DebugUart { regs }
    }

    pub fn write_byte(&mut self, byte: u8) {
        while self.regs.isr().read().txe().bit_is_clear() {}
        self.regs
            .tdr()
            .write(|w| unsafe { w.tdr().bits(byte as u16) });
    }

    #[allow(dead_code)]
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }
}

impl core::fmt::Write for DebugUart<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if b == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(b);
        }
        Ok(())
    }
}
