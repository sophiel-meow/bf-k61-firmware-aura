
use kd32f328_pac::{gpioa, gpiof};

// LCD CS=PB12, RST=PB8, DC=PC15
pub fn init_lcd_control_pins(gpiob: &gpiof::RegisterBlock, gpioc: &gpiof::RegisterBlock) {
    gpiob.otyper().modify(|_, w| w.ot12().clear_bit().ot8().clear_bit());
    gpiob
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr12().bits(0b11).ospeedr8().bits(0b11) });
    gpiob
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr12().bits(0b00).pupdr8().bits(0b00) });
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder12().bits(0b01).moder8().bits(0b01) }); // gp output

    gpioc.otyper().modify(|_, w| w.ot15().clear_bit());
    gpioc
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr15().bits(0b11) });
    gpioc.pupdr().modify(|_, w| unsafe { w.pupdr15().bits(0b00) });
    gpioc.moder().modify(|_, w| unsafe { w.moder15().bits(0b01) }); // gp output
}

pub fn set_lcd_cs(gpiob: &gpiof::RegisterBlock, high: bool) {
    if high {
        gpiob.bsrr().write(|w| w.bs12().set_bit());
    } else {
        gpiob.brr().write(|w| w.br12().set_bit());
    }
}

pub fn set_lcd_reset(gpiob: &gpiof::RegisterBlock, high: bool) {
    if high {
        gpiob.bsrr().write(|w| w.bs8().set_bit());
    } else {
        gpiob.brr().write(|w| w.br8().set_bit());
    }
}

pub fn set_lcd_dc(gpioc: &gpiof::RegisterBlock, data_mode: bool) {
    if data_mode {
        gpioc.bsrr().write(|w| w.bs15().set_bit());
    } else {
        gpioc.brr().write(|w| w.br15().set_bit());
    }
}

// LCD SPI SCK=PB13, MOSI=PB15 (AF0 = SPI2)
pub fn init_lcd_spi_pins(gpiob: &gpiof::RegisterBlock) {
    gpiob
        .afrh()
        .modify(|_, w| unsafe { w.afrh13().bits(0).afrh15().bits(0) });
    gpiob
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr13().bits(0b11).ospeedr15().bits(0b11) });
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder13().bits(0b10).moder15().bits(0b10) }); // AF mode
}

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

// LCD backlight: PF7
pub fn init_lcd_backlight_pin(gpiof: &gpiof::RegisterBlock) {
    gpiof.otyper().modify(|_, w| w.ot7().clear_bit()); // push-pull
    gpiof
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr7().bits(0b11) });
    gpiof.pupdr().modify(|_, w| unsafe { w.pupdr7().bits(0b00) });
    gpiof.moder().modify(|_, w| unsafe { w.moder7().bits(0b01) }); // gp output
}

pub fn set_lcd_backlight(gpiof: &gpiof::RegisterBlock, on: bool) {
    if on {
        gpiof.bsrr().write(|w| w.bs7().set_bit());
    } else {
        gpiof.brr().write(|w| w.br7().set_bit());
    }
}


