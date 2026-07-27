//! shim for embedded-hal

use cortex_m::peripheral::SYST;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{Error, ErrorType, OutputPin};

pub struct ClosurePin<F: FnMut(bool)>(pub F);

#[derive(Debug)]
pub struct NeverError;

impl Error for NeverError {
    fn kind(&self) -> embedded_hal::digital::ErrorKind {
        embedded_hal::digital::ErrorKind::Other
    }
}

impl<F: FnMut(bool)> ErrorType for ClosurePin<F> {
    type Error = NeverError;
}

impl<F: FnMut(bool)> OutputPin for ClosurePin<F> {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        (self.0)(false);
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        (self.0)(true);
        Ok(())
    }
}

pub struct SystDelay<'a>(pub &'a mut SYST);

impl DelayNs for SystDelay<'_> {
    fn delay_ns(&mut self, ns: u32) {
        let ms = ns / 1_000_000 + 1; // ceiling to 1ms
        crate::hal::delay::ms(self.0, ms);
    }
}
