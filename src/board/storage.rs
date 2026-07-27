use kd32f328_pac::gpioa;

// SPI NOR flash: CS=PA2 (soft NSS), SCK=PA5, MISO=PA6, MOSI=PA7 (AF0 = SPI1)
pub fn init_norflash_pins(gpioa: &gpioa::RegisterBlock) {
    gpioa.otyper().modify(|_, w| w.ot2().clear_bit());
    gpioa.ospeedr().modify(|_, w| unsafe {
        w.ospeedr2()
            .bits(0b11)
            .ospeedr5()
            .bits(0b11)
            .ospeedr6()
            .bits(0b11)
            .ospeedr7()
            .bits(0b11)
    });
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr2().bits(0b00) });
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder2().bits(0b01) }); // CS: gp output

    gpioa
        .afrl()
        .modify(|_, w| unsafe { w.afrl5().bits(0).afrl6().bits(0).afrl7().bits(0) });
    gpioa.moder().modify(|_, w| unsafe {
        w.moder5()
            .bits(0b10)
            .moder6()
            .bits(0b10)
            .moder7()
            .bits(0b10)
    }); // AF mode
}

pub fn set_norflash_cs(gpioa: &gpioa::RegisterBlock, high: bool) {
    if high {
        gpioa.bsrr().write(|w| w.bs2().set_bit());
    } else {
        gpioa.brr().write(|w| w.br2().set_bit());
    }
}
