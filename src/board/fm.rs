use kd32f328_pac::gpioa;

// RDA5807M I2C bus: SCL=PA4, SDA=PA12, bit-banged
pub fn init_i2c_pins(gpioa: &gpioa::RegisterBlock) {
    gpioa
        .otyper()
        .modify(|_, w| w.ot4().clear_bit().ot12().clear_bit());
    gpioa
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr4().bits(0b11).ospeedr12().bits(0b11) });
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr4().bits(0b00).pupdr12().bits(0b00) });
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder4().bits(0b01).moder12().bits(0b01) }); // gp output
}

pub fn set_i2c_scl(gpioa: &gpioa::RegisterBlock, high: bool) {
    if high {
        gpioa.bsrr().write(|w| w.bs4().set_bit());
    } else {
        gpioa.brr().write(|w| w.br4().set_bit());
    }
}

pub fn set_i2c_sda(gpioa: &gpioa::RegisterBlock, high: bool) {
    if high {
        gpioa.bsrr().write(|w| w.bs12().set_bit());
    } else {
        gpioa.brr().write(|w| w.br12().set_bit());
    }
}

pub fn set_i2c_sda_input(gpioa: &gpioa::RegisterBlock) {
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr12().bits(0b01) }); // pull-up
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder12().bits(0b00) });
}

pub fn set_i2c_sda_output(gpioa: &gpioa::RegisterBlock) {
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder12().bits(0b01) });
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr12().bits(0b00) });
}

pub fn read_i2c_sda(gpioa: &gpioa::RegisterBlock) -> bool {
    gpioa.idr().read().idr12().bit_is_set()
}
