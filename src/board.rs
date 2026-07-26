
use kd32f328_pac::{gpioa, gpiof};

pub fn init_ptt_rxd_pin(gpioa: &gpioa::RegisterBlock) {
    gpioa.pupdr().modify(|_, w| unsafe { w.pupdr10().bits(0b01) }); // pull-up
    gpioa.afrh().modify(|_, w| unsafe { w.afrh10().bits(1) }); // AF1 = USART1_RX
    gpioa.moder().modify(|_, w| unsafe { w.moder10().bits(0b10) }); // AF mode
}

pub fn read_ptt(gpioa: &gpioa::RegisterBlock) -> bool {
    gpioa.idr().read().idr10().bit_is_set()
}

pub fn init_flashlight_led(gpiob: &gpiof::RegisterBlock) {
    gpiob.otyper().modify(|_, w| w.ot7().clear_bit()); // push-pull
    gpiob
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr7().bits(0b11) });
    gpiob.pupdr().modify(|_, w| unsafe { w.pupdr7().bits(0b00) });
    gpiob.moder().modify(|_, w| unsafe { w.moder7().bits(0b01) }); // gp output
}

pub fn set_flashlight_led(gpiob: &gpiof::RegisterBlock, on: bool) {
    if on {
        gpiob.bsrr().write(|w| w.bs7().set_bit());
    } else {
        gpiob.brr().write(|w| w.br7().set_bit());
    }
}

pub fn init_debug_uart_tx_pin(gpioa: &gpioa::RegisterBlock) {
    gpioa.afrh().modify(|_, w| unsafe { w.afrh9().bits(1) }); // AF1 = USART1_TX
    gpioa.moder().modify(|_, w| unsafe { w.moder9().bits(0b10) }); // AF mode
}


