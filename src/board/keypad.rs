use kd32f328_pac::gpiof;

// Keypad matrix:
// 5 row outputs on PB0/PB4/PB9/PB10/PB11
// 4 column inputs on PC13/PC14/PF6/PB14
pub fn init_keypad_pins(
    gpiob: &gpiof::RegisterBlock,
    gpioc: &gpiof::RegisterBlock,
    gpiof: &gpiof::RegisterBlock,
) {
    gpiob.otyper().modify(|_, w| {
        w.ot0()
            .clear_bit()
            .ot4()
            .clear_bit()
            .ot9()
            .clear_bit()
            .ot10()
            .clear_bit()
            .ot11()
            .clear_bit()
    });
    gpiob.ospeedr().modify(|_, w| unsafe {
        w.ospeedr0()
            .bits(0b01)
            .ospeedr4()
            .bits(0b01)
            .ospeedr9()
            .bits(0b01)
            .ospeedr10()
            .bits(0b01)
            .ospeedr11()
            .bits(0b01)
    });
    gpiob.pupdr().modify(|_, w| unsafe {
        w.pupdr0()
            .bits(0b00)
            .pupdr4()
            .bits(0b00)
            .pupdr9()
            .bits(0b00)
            .pupdr10()
            .bits(0b00)
            .pupdr11()
            .bits(0b00)
    });
    gpiob.moder().modify(|_, w| unsafe {
        w.moder0()
            .bits(0b01)
            .moder4()
            .bits(0b01)
            .moder9()
            .bits(0b01)
            .moder10()
            .bits(0b01)
            .moder11()
            .bits(0b01)
    }); // rows: gp output
    set_keypad_rows_idle(gpiob);

    gpiob
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr14().bits(0b01) });
    gpiob
        .moder()
        .modify(|_, w| unsafe { w.moder14().bits(0b00) }); // input

    gpioc
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr13().bits(0b01).pupdr14().bits(0b01) });
    gpioc
        .moder()
        .modify(|_, w| unsafe { w.moder13().bits(0b00).moder14().bits(0b00) }); // input

    gpiof
        .pupdr()
        .modify(|_, w| unsafe { w.pupdr6().bits(0b01) });
    gpiof
        .moder()
        .modify(|_, w| unsafe { w.moder6().bits(0b00) }); // input
}

pub fn set_keypad_rows_idle(gpiob: &gpiof::RegisterBlock) {
    gpiob.bsrr().write(|w| {
        w.bs0()
            .set_bit()
            .bs4()
            .set_bit()
            .bs9()
            .set_bit()
            .bs10()
            .set_bit()
            .bs11()
            .set_bit()
    });
}

/// `phase` 0 = no row driven (side-key probe). 1..=5 drives exactly one row
/// low, in this board's physical row-to-pin order (not row-index order).
pub fn set_keypad_row(gpiob: &gpiof::RegisterBlock, phase: u8) {
    set_keypad_rows_idle(gpiob);
    match phase {
        1 => gpiob.brr().write(|w| w.br0().set_bit()),
        2 => gpiob.brr().write(|w| w.br4().set_bit()),
        3 => gpiob.brr().write(|w| w.br10().set_bit()),
        4 => gpiob.brr().write(|w| w.br11().set_bit()),
        5 => gpiob.brr().write(|w| w.br9().set_bit()),
        _ => return,
    };
}

/// Returns the index (0..=3) of the one column reading low, if any.
pub fn read_keypad_column(
    gpiob: &gpiof::RegisterBlock,
    gpioc: &gpiof::RegisterBlock,
    gpiof: &gpiof::RegisterBlock,
) -> Option<u8> {
    if gpioc.idr().read().idr13().bit_is_clear() {
        Some(0)
    } else if gpioc.idr().read().idr14().bit_is_clear() {
        Some(1)
    } else if gpiof.idr().read().idr6().bit_is_clear() {
        Some(2)
    } else if gpiob.idr().read().idr14().bit_is_clear() {
        Some(3)
    } else {
        None
    }
}
