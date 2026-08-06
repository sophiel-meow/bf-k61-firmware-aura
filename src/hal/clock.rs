use kd32f328_pac::{flash, rcc};

pub const SYSCLK_HZ: u32 = 96_000_000;

/// HSE(16 MHz), PREDIV(/1), PLL*6 = 96 MHz.
pub fn setup_pll(rcc: &rcc::RegisterBlock, flash: &flash::RegisterBlock) {
    flash
        .acr()
        .write(|w| unsafe { w.prftbe().set_bit().latency().bits(0b0010) });

    rcc.cr().modify(|_, w| w.hseon().set_bit());
    while rcc.cr().read().hserdy().bit_is_clear() {}

    rcc.cfgr2().write(|w| unsafe { w.prediv().bits(0) });

    rcc.cfgr().write(|w| unsafe {
        w.pllsrc().bits(0b10); // HSE PREDIV clock selected as PLL entry
        w.pllmul().bits(4) // x6 (field value = multiplier - 2)
    });

    rcc.cr().modify(|_, w| w.pllon().set_bit());
    while rcc.cr().read().pllrdy().bit_is_clear() {}

    rcc.cfgr().modify(|_, w| unsafe { w.sw().bits(0b10) });
    while rcc.cfgr().read().sws().bits() != 0b10 {}
}

pub fn enable_peripheral_clocks(rcc: &rcc::RegisterBlock) {
    rcc.ahbenr().modify(|_, w| {
        w.iopaen()
            .set_bit()
            .iopben()
            .set_bit()
            .iopcen()
            .set_bit()
            .iopfen()
            .set_bit()
    });
    rcc.apb2enr()
        .modify(|_, w| w.usart1en().set_bit().spi1en().set_bit().adcen().set_bit());
    rcc.apb1enr()
        .modify(|_, w| w.spi2en().set_bit().tim6en().set_bit().tim14en().set_bit());
}
