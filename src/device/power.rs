use crate::board;
use crate::hal::delay;
use cortex_m::peripheral::{SCB, SYST};
use kd32f328_pac::gpioa;

pub struct Power {
    gpioa: &'static gpioa::RegisterBlock,
}

impl Power {
    pub fn new(gpioa: &'static gpioa::RegisterBlock) -> Self {
        Self { gpioa }
    }

    /// Latch power on. Must be called as early as possible in `main()`,
    /// before the power switch bounces back open.
    pub fn latch_on(&self) {
        board::set_power_latch(self.gpioa, true);
    }

    /// Debounced power-off check. Call in the main loop; when it returns
    /// `true` the user held the switch long enough to confirm shutdown.
    pub fn debounced_off(&self, syst: &mut SYST) -> bool {
        if board::power_switch_off(self.gpioa) {
            delay::ms(syst, 50);
            board::power_switch_off(self.gpioa)
        } else {
            false
        }
    }

    pub fn switch_off_raw(&self) -> bool {
        board::power_switch_off(self.gpioa)
    }

    /// Full shutdown sequence: RF off, release power latch, wait for
    /// capacitors to drain, then reset (which will re-latch on next
    /// power-on if the switch is still held).
    pub fn shutdown(&self, syst: &mut SYST) -> ! {
        board::set_power_latch(self.gpioa, false);
        delay::ms(syst, 500);
        SCB::sys_reset();
    }
}
