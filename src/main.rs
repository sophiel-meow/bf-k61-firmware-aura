#![no_std]
#![no_main]

use cortex_m::peripheral::syst::SystClkSource;
use cortex_m::peripheral::SYST;
use cortex_m_rt::entry;
use kd32f328_pac::Peripherals;
use panic_halt as _;

mod debounce;
use debounce::Debouncer;

/// 96 MHz as shipped; used for SysTick reload value.
const SYSCLK_HZ: u32 = 96_000_000;

/// HSI(8 MHz), PREDIV(/1),  PLL*12 = 96 MHz.
fn setup_clocks(
    rcc: &kd32f328_pac::rcc::RegisterBlock,
    flash: &kd32f328_pac::flash::RegisterBlock,
) {
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

fn enable_gpio_clocks(rcc: &kd32f328_pac::rcc::RegisterBlock) {
    rcc.ahbenr()
        .modify(|_, w| w.iopaen().set_bit().iopben().set_bit());
}

fn configure_gpioa_pin10_input_pullup(gpioa: &kd32f328_pac::gpioa::RegisterBlock) {
    gpioa.pupdr().modify(|_, w| unsafe { w.pupdr10().bits(0b01) });
    gpioa.moder().modify(|_, w| unsafe { w.moder10().bits(0b00) });
}

fn configure_gpiob_pin7_output_pushpull(gpiob: &kd32f328_pac::gpiof::RegisterBlock) {
    gpiob
        .otyper()
        .modify(|_, w| w.ot7().clear_bit()); // push-pull
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

fn read_pa10(gpioa: &kd32f328_pac::gpioa::RegisterBlock) -> bool {
    gpioa.idr().read().idr10().bit_is_set()
}

fn set_pb7(pin_block: &kd32f328_pac::gpiof::RegisterBlock, on: bool) {
    if on {
        pin_block.bsrr().write(|w| w.bs7().set_bit());
    } else {
        pin_block.brr().write(|w| w.br7().set_bit());
    }
}

fn delay_ms(syst: &mut SYST, ms: u32) {
    let reload = SYSCLK_HZ / 1000 - 1;
    syst.set_clock_source(SystClkSource::Core);
    syst.set_reload(reload);
    for _ in 0..ms {
        syst.clear_current();
        syst.enable_counter();
        while !syst.has_wrapped() {}
        syst.disable_counter();
    }
}

#[entry]
fn main() -> ! {
    let _dp = unsafe { Peripherals::steal() };

    let rcc_block = unsafe { &*kd32f328_pac::Rcc::ptr() };
    let gpioa_block = unsafe { &*kd32f328_pac::Gpioa::ptr() };
    let gpiob_block = unsafe { &*kd32f328_pac::Gpiob::ptr() };
    let flash_block = unsafe { &*kd32f328_pac::Flash::ptr() };

    setup_clocks(rcc_block, flash_block);
    enable_gpio_clocks(rcc_block);
    configure_gpioa_pin10_input_pullup(gpioa_block);
    configure_gpiob_pin7_output_pushpull(gpiob_block);

    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut debouncer = Debouncer::new(read_pa10(gpioa_block));
    let mut light_on = false;

    loop {
        if let Some(level) = debouncer.sample(read_pa10(gpioa_block)) {
            if !level {
                light_on = !light_on;
                set_pb7(gpiob_block, light_on);
            }
        }
        delay_ms(&mut cp.SYST, 5);
    }
}
