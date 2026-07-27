use kd32f328_pac::{gpioa, gpiof};

// FD6818 SCN=PB3, SCK=PB5, SDA=PB6(bi)
pub fn init_fd6818_pins(gpiob: &gpiof::RegisterBlock) {
    gpiob
        .otyper()
        .modify(|_, w| w.ot3().clear_bit().ot5().clear_bit().ot6().clear_bit());
    gpiob.ospeedr().modify(|_, w| unsafe {
        w.ospeedr3()
            .bits(0b11)
            .ospeedr5()
            .bits(0b11)
            .ospeedr6()
            .bits(0b11)
    });
    gpiob.pupdr().modify(|_, w| unsafe {
        w.pupdr3()
            .bits(0b00)
            .pupdr5()
            .bits(0b00)
            .pupdr6()
            .bits(0b01)
    });
    gpiob.moder().modify(|_, w| unsafe {
        w.moder3()
            .bits(0b01)
            .moder5()
            .bits(0b01)
            .moder6()
            .bits(0b01)
    });
}

pub fn set_fd6818_scn(gpiob: &gpiof::RegisterBlock, high: bool) {
    if high {
        gpiob.bsrr().write(|w| w.bs3().set_bit());
    } else {
        gpiob.brr().write(|w| w.br3().set_bit());
    }
}

pub fn set_fd6818_sck(gpiob: &gpiof::RegisterBlock, high: bool) {
    if high {
        gpiob.bsrr().write(|w| w.bs5().set_bit());
    } else {
        gpiob.brr().write(|w| w.br5().set_bit());
    }
}

pub fn set_fd6818_sda(gpiob: &gpiof::RegisterBlock, high: bool) {
    if high {
        gpiob.bsrr().write(|w| w.bs6().set_bit());
    } else {
        gpiob.brr().write(|w| w.br6().set_bit());
    }
}

pub fn set_fd6818_sda_input(gpiob: &gpiof::RegisterBlock) {
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder6().bits(0b00) });
}

pub fn set_fd6818_sda_output(gpiob: &gpiof::RegisterBlock) {
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder6().bits(0b01) });
}

pub fn read_fd6818_sda(gpiob: &gpiof::RegisterBlock) -> bool {
    gpiob.idr().read().idr6().bit_is_set()
}

/// RX front-end band select: PA13 = UHF path, PA14 = VHF path. Exactly one
/// is driven high while receiving; both are low for TX and for power-down
pub fn init_rx_band_pins(gpioa: &gpioa::RegisterBlock) {
    gpioa
        .otyper()
        .modify(|_, w| w.ot13().clear_bit().ot14().clear_bit()); // push-pull
    gpioa
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr13().bits(0b11).ospeedr14().bits(0b11) });
    gpioa
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr13().bits(0b00).pupdr14().bits(0b00) });
    gpioa
        .moder()
        .modify(|_, w| unsafe { w.moder13().bits(0b01).moder14().bits(0b01) }); // gp output
    set_rx_band_off(gpioa);
}

pub fn set_rx_band_uhf(gpioa: &gpioa::RegisterBlock) {
    gpioa.brr().write(|w| w.br14().set_bit());
    gpioa.bsrr().write(|w| w.bs13().set_bit());
}

pub fn set_rx_band_vhf(gpioa: &gpioa::RegisterBlock) {
    gpioa.brr().write(|w| w.br13().set_bit());
    gpioa.bsrr().write(|w| w.bs14().set_bit());
}

pub fn set_rx_band_off(gpioa: &gpioa::RegisterBlock) {
    gpioa.brr().write(|w| w.br13().set_bit().br14().set_bit());
}

// Speaker amp enable/mute: PB2
pub fn init_speaker_switch_pin(gpiob: &gpiof::RegisterBlock) {
    gpiob.otyper().modify(|_, w| w.ot2().clear_bit()); // push-pull
    gpiob
        .ospeedr()
        .modify(|_, w| unsafe { w.ospeedr2().bits(0b11) });
    gpiob
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr2().bits(0b00) });
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder2().bits(0b01) }); // gp output
}

pub fn set_speaker_switch(gpiob: &gpiof::RegisterBlock, on: bool) {
    if on {
        gpiob.bsrr().write(|w| w.bs2().set_bit());
    } else {
        gpiob.brr().write(|w| w.br2().set_bit());
    }
}
