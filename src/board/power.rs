use kd32f328_pac::gpioa;

// Power latch/detect: PA8 = POW_EN, PA15 = POW_DET
pub fn init_power_pins(gpioa: &gpioa::RegisterBlock) {
    gpioa.otyper().modify(|_, w| w.ot8().clear_bit()); // push-pull
    gpioa
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr8().bits(0b11) });
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr8().bits(0b00).pupdr15().bits(0b01) });
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder8().bits(0b01) }); // gp output
    set_power_latch(gpioa, true);
}

pub fn set_power_latch(gpioa: &gpioa::RegisterBlock, on: bool) {
    if on {
        gpioa.bsrr().write(|w| w.bs8().set_bit());
    } else {
        gpioa.brr().write(|w| w.br8().set_bit());
    }
}

/// true = the physical power switch currently reads as OFF.
pub fn power_switch_off(gpioa: &gpioa::RegisterBlock) -> bool {
    gpioa.idr().read().idr15().bit_is_set()
}

/// Battery sense: PA1, ADC1 channel 1
pub fn init_battery_adc_pin(gpioa: &gpioa::RegisterBlock) {
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder1().bits(0b11) }); // analog
}
