use kd32f328_pac::spi1;

#[derive(Clone, Copy)]
pub enum ClockMode {
    /// CPOL=0, CPHA=0
    Mode0,
    /// CPOL=0, CPHA=1
    Mode1,
    /// CPOL=1, CPHA=0
    Mode2,
    /// CPOL=1, CPHA=1
    Mode3,
}

pub struct SpiBus<'a> {
    regs: &'a spi1::RegisterBlock,
}

impl<'a> SpiBus<'a> {
    /// `baud_div`: SPI clock prescaler relative to the APBx clock.
    /// Valid values are 2, 4, 8, 16, 32, 64, 128, and 256.
    /// 
    /// NSS is managed by the caller through a GPIO pin (soft NSS).
    pub fn new(regs: &'a spi1::RegisterBlock, mode: ClockMode, baud_div: u16) -> Self {
        let (cpol, cpha) = match mode {
            ClockMode::Mode0 => (false, false),
            ClockMode::Mode1 => (false, true),
            ClockMode::Mode2 => (true, false),
            ClockMode::Mode3 => (true, true),
        };
        let br = match baud_div {
            2 => 0b000,
            4 => 0b001,
            8 => 0b010,
            16 => 0b011,
            32 => 0b100,
            64 => 0b101,
            128 => 0b110,
            _ => 0b111, // 256
        };

        // 8-bit frame; FRXTH so RXNE flags after every single received byte
        // instead of waiting for two bytes to pack into a 16-bit slot.
        regs.cr2()
            .write(|w| unsafe { w.ds().bits(0b0111).frxth().set_bit() });

        regs.cr1().write(|w| unsafe {
            w.mstr().set_bit();
            w.cpol().bit(cpol);
            w.cpha().bit(cpha);
            w.br().bits(br);
            w.ssm().set_bit(); // soft NSS
            w.ssi().set_bit();
            w.lsbfirst().clear_bit(); // MSB first
            w.spe().set_bit()
        });

        SpiBus { regs }
    }

    pub fn write_byte(&mut self, byte: u8) {
        while self.regs.sr().read().txe().bit_is_clear() {}
        // With the frame size configured to 8 bits,
        // a single 16/32-bit register write is interpreted
        // as multiple bytes and packed into the transmit FIFO, which causes all
        // following bytes to become misaligned by one byte. The DR register must
        // therefore be accessed with byte-width writes.
        unsafe {
            core::ptr::write_volatile(self.regs.dr().as_ptr() as *mut u8, byte);
        }
        while self.regs.sr().read().bsy().bit_is_set() {}
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }

    pub fn read_byte(&mut self) -> u8 {
        // for embedded-hal::SpiBus interface
        self.write_byte(0x00);
        self.regs.dr().read().dr().bits() as u8
    }

    pub fn transfer_byte(&mut self, byte: u8) -> u8 {
        while self.regs.sr().read().txe().bit_is_clear() {}
        unsafe {
            core::ptr::write_volatile(self.regs.dr().as_ptr() as *mut u8, byte);
        }
        while self.regs.sr().read().rxne().bit_is_clear() {}
        let rx = unsafe { core::ptr::read_volatile(self.regs.dr().as_ptr() as *const u8) };
        while self.regs.sr().read().bsy().bit_is_set() {}
        rx
    }
}

impl embedded_hal::spi::ErrorType for SpiBus<'_> {
    type Error = core::convert::Infallible;
}

impl embedded_hal::spi::SpiBus<u8> for SpiBus<'_> {
    fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for w in words.iter_mut() {
            *w = self.read_byte();
        }
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        self.write_bytes(words);
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        let n = read.len().max(write.len());
        for i in 0..n {
            let tx = write.get(i).copied().unwrap_or(0x00);
            self.write_byte(tx);
            if let Some(rx) = read.get_mut(i) {
                *rx = self.regs.dr().read().dr().bits() as u8;
            }
        }
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for w in words.iter_mut() {
            self.write_byte(*w);
            *w = self.regs.dr().read().dr().bits() as u8;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        while self.regs.sr().read().bsy().bit_is_set() {}
        Ok(())
    }
}
