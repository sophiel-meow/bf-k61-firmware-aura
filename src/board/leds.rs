use kd32f328_pac::{gpioa, gpiof};

pub fn init_flashlight_led(gpiob: &gpiof::RegisterBlock) {
    gpiob.otyper().modify(|_, w| w.ot7().clear_bit()); // push-pull
    gpiob
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr7().bits(0b11) });
    gpiob
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr7().bits(0b00) });
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder7().bits(0b01) }); // gp output
}

pub fn set_flashlight_led(gpiob: &gpiof::RegisterBlock, on: bool) {
    if on {
        gpiob.bsrr().write(|w| w.bs7().set_bit());
    } else {
        gpiob.brr().write(|w| w.br7().set_bit());
    }
}

// RX indicator LED (G_LED): PA3, push-pull, active-high
pub fn init_rx_led_pin(gpioa: &gpioa::RegisterBlock) {
    gpioa.otyper().modify(|_, w| w.ot3().clear_bit()); // push-pull
    gpioa
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr3().bits(0b11) });
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr3().bits(0b00) });
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder3().bits(0b01) }); // gp output
    set_rx_led(gpioa, false);
}

pub fn set_rx_led(gpioa: &gpioa::RegisterBlock, on: bool) {
    if on {
        gpioa.bsrr().write(|w| w.bs3().set_bit());
    } else {
        gpioa.brr().write(|w| w.br3().set_bit());
    }
}
