#![no_std]
#![no_main]

use core::fmt::Write as _;
use cortex_m_rt::entry;
use kd32f328_pac::Peripherals;
use panic_halt as _;

mod board;
mod clock;
mod debounce;
mod delay;
mod uart;

use debounce::Debouncer;

#[entry]
fn main() -> ! {
    let _dp = unsafe { Peripherals::steal() };

    let rcc = unsafe { &*kd32f328_pac::Rcc::ptr() };
    let gpioa = unsafe { &*kd32f328_pac::Gpioa::ptr() };
    let gpiob = unsafe { &*kd32f328_pac::Gpiob::ptr() };
    let flash = unsafe { &*kd32f328_pac::Flash::ptr() };
    let usart1 = unsafe { &*kd32f328_pac::Usart1::ptr() };

    clock::setup_pll(rcc, flash);
    clock::enable_peripheral_clocks(rcc);

    board::init_ptt_rxd_pin(gpioa);
    board::init_flashlight_led(gpiob);
    board::init_debug_uart_tx_pin(gpioa);

    let mut dbg = uart::DebugUart::new(usart1, clock::SYSCLK_HZ, 115_200);
    writeln!(dbg, "bfk6-fw boot, sysclk={}Hz", clock::SYSCLK_HZ).ok();

    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut debouncer = Debouncer::new(board::read_ptt(gpioa));
    let mut light_on = false;

    loop {
        if let Some(level) = debouncer.sample(board::read_ptt(gpioa)) {
            if !level {
                light_on = !light_on;
                board::set_flashlight_led(gpiob, light_on);
                writeln!(dbg, "PTT pressed, led={}", light_on).ok();
            }
        }
        delay::ms(&mut cp.SYST, 5);
    }
}
