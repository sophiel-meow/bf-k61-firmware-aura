use cortex_m::peripheral::syst::SystClkSource;
use cortex_m::peripheral::SYST;

use crate::clock::SYSCLK_HZ;

pub fn ms(syst: &mut SYST, ms: u32) {
    let reload = SYSCLK_HZ / 1000 - 1;
    syst.set_clock_source(SystClkSource::Core);
    syst.set_reload(reload);
    for _ in 0..ms {
        syst.clear_current();
        syst.enable_counter();
        while !syst.has_wrapped() {}
        syst.disable_counter();
    }
}
