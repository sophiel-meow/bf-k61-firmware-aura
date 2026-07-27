use kd32f328_pac::{flash, rcc};

/// 96 MHz as shipped
pub const SYSCLK_HZ: u32 = 96_000_000;

/// HSI(8 MHz), PREDIV(/1), PLL*12 = 96 MHz.
pub fn setup_pll(rcc: &rcc::RegisterBlock, flash: &flash::RegisterBlock) {
    flash
        .acr()
        .write(|w| unsafe { w.prftbe().set_bit().latency().bits(0b0010) });

    rcc.cfgr2().write(|w| unsafe { w.prediv().bits(0) });

    rcc.cfgr().write(|w| unsafe {
        w.pllsrc().bits(0b01);
        w.pllmul().bits(0b1010)
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
    rcc.apb2enr().modify(|_, w| w.usart1en().set_bit());
    rcc.apb1enr().modify(|_, w| w.spi2en().set_bit());
}

