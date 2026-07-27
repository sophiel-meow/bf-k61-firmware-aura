use kd32f328_pac::gpioa;

pub fn init_ptt_rxd_pin(gpioa: &gpioa::RegisterBlock) {
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr10().bits(0b01) }); // pull-up
    gpioa.afrh().modify(|_, w| unsafe { w.afrh10().bits(1) }); // AF1 = USART1_RX
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder10().bits(0b10) }); // AF mode
}

pub fn read_ptt(gpioa: &gpioa::RegisterBlock) -> bool {
    gpioa.idr().read().idr10().bit_is_set()
}

pub fn init_debug_uart_tx_pin(gpioa: &gpioa::RegisterBlock) {
    gpioa.afrh().modify(|_, w| unsafe { w.afrh9().bits(1) }); // AF1 = USART1_TX
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder9().bits(0b10) }); // AF mode
}
