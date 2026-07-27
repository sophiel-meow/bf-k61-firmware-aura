#![no_std]
#![no_main]

use core::fmt::Write as _;
use cortex_m_rt::entry;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    prelude::*,
    text::Text,
};
use kd32f328_pac::Peripherals;
use panic_halt as _;

mod board;
mod clock;
mod debounce;
mod delay;
mod display_spec;
mod fd6818;
mod hal_shim;
mod spi;
mod uart;

use debounce::Debouncer;
use hal_shim::{ClosurePin, SystDelay};

#[entry]
fn main() -> ! {
    let _dp = unsafe { Peripherals::steal() };

    let rcc = unsafe { &*kd32f328_pac::Rcc::ptr() };
    let gpioa = unsafe { &*kd32f328_pac::Gpioa::ptr() };
    let gpiob = unsafe { &*kd32f328_pac::Gpiob::ptr() };
    let gpioc = unsafe { &*kd32f328_pac::Gpioc::ptr() };
    let gpiof = unsafe { &*kd32f328_pac::Gpiof::ptr() };
    let flash = unsafe { &*kd32f328_pac::Flash::ptr() };
    let usart1 = unsafe { &*kd32f328_pac::Usart1::ptr() };
    let spi2 = unsafe { &*kd32f328_pac::Spi2::ptr() };

    clock::setup_pll(rcc, flash);
    clock::enable_peripheral_clocks(rcc);

    board::init_ptt_rxd_pin(gpioa);
    board::init_flashlight_led(gpiob);
    board::init_debug_uart_tx_pin(gpioa);
    board::init_lcd_control_pins(gpiob, gpioc);
    board::init_lcd_spi_pins(gpiob);
    board::init_lcd_backlight_pin(gpiof);
    board::set_lcd_backlight(gpiof, true);
    board::init_fd6818_pins(gpiob);

    let mut dbg = uart::DebugUart::new(usart1, clock::SYSCLK_HZ, 115_200);
    writeln!(dbg, "bfk6-fw boot, sysclk={}Hz", clock::SYSCLK_HZ).ok();

    let mut cp = cortex_m::Peripherals::take().unwrap();

    let spi_bus = spi::SpiBus::new(spi2, spi::ClockMode::Mode3, 8);
    let cs_pin = ClosurePin(|level| board::set_lcd_cs(gpiob, level));
    let dc_pin = ClosurePin(|level| board::set_lcd_dc(gpioc, level));
    let mut rst_pin = ClosurePin(|level| board::set_lcd_reset(gpiob, level));

    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi_bus, cs_pin)
        .unwrap_or_else(|_| unreachable!());
    let interface = display_interface_spi::SPIInterface::new(spi_device, dc_pin);

    let mut page_buffer: st7565::GraphicsPageBuffer<128, 8> = st7565::GraphicsPageBuffer::new();
    let mut lcd =
        st7565::ST7565::new(interface, display_spec::Sc5260Spec).into_graphics_mode(&mut page_buffer);

    {
        let mut syst_delay = SystDelay(&mut cp.SYST);
        lcd.reset(&mut rst_pin, &mut syst_delay).ok();
    }
    lcd.flush().ok();
    lcd.set_display_on(true).ok();
    writeln!(dbg, "lcd init done").ok();

    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(&mut lcd)
        .ok();
    Text::new("Hello, World!", Point::new(24,32),
              MonoTextStyle::new(&FONT_6X10, BinaryColor::On))
        .draw(&mut lcd)
        .ok();

    lcd.flush().ok();

    let mut rfic = fd6818::Fd6818::new(gpiob);
    let scratch_addr = 0x71;
    let scratch_value = 0xA5A5u16;
    rfic.write_reg(&mut cp.SYST, scratch_addr, scratch_value);
    let readback = rfic.read_reg(&mut cp.SYST, scratch_addr);
    writeln!(
        dbg,
        "fd6818 scratch reg 0x{:02x}: wrote {:#06x}, read back {:#06x}, {}",
        scratch_addr,
        scratch_value,
        readback,
        if readback == scratch_value { "OK" } else { "MISMATCH" }
    )
    .ok();

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
