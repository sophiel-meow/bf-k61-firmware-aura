#![no_std]
#![no_main]

use core::fmt::Write as _;
use cortex_m::peripheral::SCB;
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

mod adc;
mod board;
mod clock;
mod debounce;
mod delay;
mod display_spec;
mod fd6818;
mod hal_shim;
mod i2c;
mod keypad;
mod norflash;
mod radio;
mod rda5807;
mod spi;
mod uart;

use debounce::Debouncer;
use hal_shim::{ClosurePin, SystDelay};

const TEST_FREQ_HZ: u32 = 439_500_000;
const FM_TEST_FREQ_KHZ: u32 = 107_600;

fn checksum32(buf: &[u8]) -> u32 {
    buf.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32))
}

struct TextBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> TextBuf<N> {
    fn new() -> Self {
        TextBuf {
            buf: [0; N],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> core::fmt::Write for TextBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let space = N - self.len;
        let n = bytes.len().min(space);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
        Ok(())
    }
}

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
    let spi1 = unsafe { &*kd32f328_pac::Spi1::ptr() };
    let spi2 = unsafe { &*kd32f328_pac::Spi2::ptr() };
    let adc_regs = unsafe { &*kd32f328_pac::Adc::ptr() };

    clock::setup_pll(rcc, flash);
    clock::enable_peripheral_clocks(rcc);

    // Latch the power-supply enable as early as possible, before the power
    // switch's own momentary contact can bounce back open.
    board::init_power_pins(gpioa);

    board::init_ptt_rxd_pin(gpioa);
    board::init_flashlight_led(gpiob);
    board::init_debug_uart_tx_pin(gpioa);
    board::init_lcd_control_pins(gpiob, gpioc);
    board::init_lcd_spi_pins(gpiob);
    board::init_lcd_backlight_pin(gpiof);
    board::set_lcd_backlight(gpiof, true);
    board::init_fd6818_pins(gpiob);
    board::init_norflash_pins(gpioa);
    board::init_speaker_switch_pin(gpiob);
    board::init_rx_band_pins(gpioa);
    board::init_i2c_pins(gpioa);
    board::init_battery_adc_pin(gpioa);
    board::init_keypad_pins(gpiob, gpioc, gpiof);
    board::init_rx_led_pin(gpioa);

    let mut batt_adc = adc::Adc::new(adc_regs);
    let mut keys = keypad::KeyManager::new(gpiob, gpioc, gpiof);

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
    let mut lcd = st7565::ST7565::new(interface, display_spec::Sc5260Spec)
        .into_graphics_mode(&mut page_buffer);

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
    Text::new(
        "Hello, World!",
        Point::new(24, 32),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
    )
    .draw(&mut lcd)
    .ok();

    lcd.flush().ok();

    let mut rfic = fd6818::Fd6818::new(gpiob);
    rfic.init(&mut cp.SYST);
    writeln!(dbg, "fd6818 init done").ok();

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
        if readback == scratch_value {
            "OK"
        } else {
            "MISMATCH"
        }
    )
    .ok();

    let flash_spi = spi::SpiBus::new(spi1, spi::ClockMode::Mode3, 4);
    let mut norflash = norflash::NorFlash::new(flash_spi, gpioa);
    let mut cal_buf = [0u8; 16];
    norflash.read_bytes(0xF210, &mut cal_buf);
    writeln!(dbg, "norflash cal block @0xf210: {:02x?}", cal_buf).ok();

    let xtal_adjust = cal_buf[6];
    rfic.set_xtal_adjust(xtal_adjust);
    writeln!(dbg, "xtal_adjust = {}", xtal_adjust).ok();

    rfic.set_audio_calibration(cal_buf[0], cal_buf[1], cal_buf[3], cal_buf[4]);
    rfic.apply_af_calibration(
        &mut cp.SYST,
        cal_buf[11],
        cal_buf[12],
        cal_buf[13],
        cal_buf[14],
    );
    writeln!(
        dbg,
        "audio cal: mic_depth={} cts_depth={} vol_wide={} vol_narrow={} af_rx=({},{}) af_tx=({},{})",
        cal_buf[0], cal_buf[1], cal_buf[3], cal_buf[4], cal_buf[11], cal_buf[12], cal_buf[13], cal_buf[14]
    )
    .ok();

    const TEST_POWER: fd6818::Power = fd6818::Power::Low;
    if let Some(pa_addr) = fd6818::Fd6818::pa_target_addr(TEST_FREQ_HZ, TEST_POWER) {
        let mut pa_byte = [0u8; 1];
        norflash.read_bytes(pa_addr, &mut pa_byte);
        rfic.set_pa_calibration(pa_byte[0]);
        writeln!(dbg, "pa_target @0x{:04x} = {}", pa_addr, pa_byte[0]).ok();
    }

    let mut radio = radio::Radio::new(
        rfic,
        gpioa,
        gpiob,
        radio::ChannelConfig {
            freq_hz: TEST_FREQ_HZ,
            wide_band: true,
            power: TEST_POWER,
            sql_level: 1,
            subaudio_tx: fd6818::SubAudio::None,
            subaudio_rx: fd6818::SubAudio::Ctcss(885),
            tail_elimination: true,
        },
    );

    radio.enter_rx(&mut cp.SYST);
    writeln!(dbg, "RX ON  @ {} Hz, watching RSSI...", TEST_FREQ_HZ).ok();

    // let mut fm = rda5807::Rda5807::new(gpioa);
    // fm.set_frequency_khz(&mut cp.SYST, FM_TEST_FREQ_KHZ);
    writeln!(dbg, "rda5807 tuned to {} kHz", FM_TEST_FREQ_KHZ).ok();

    let mut debouncer = Debouncer::new(board::read_ptt(gpioa));
    let mut transmitting = false;
    let mut rssi_tick: u8 = 0;
    let mut fm_tick: u16 = 0;
    let mut batt_tick: u16 = 0;
    let mut key_scan_toggle = false;

    const BATT_ADC_CHANNEL: u8 = 1;

    loop {
        key_scan_toggle = !key_scan_toggle;
        if key_scan_toggle {
            keys.poll(&mut cp.SYST);
            while let Some(ev) = keys.pop_event() {
                writeln!(dbg, "key: {:?} {:?}", ev.key, ev.kind).ok();
            }
        }

        if board::power_switch_off(gpioa) {
            delay::ms(&mut cp.SYST, 50);
            if board::power_switch_off(gpioa) {
                writeln!(dbg, "power switch off, shutting down").ok();
                radio.fd6818_mut().rf_off(&mut cp.SYST);
                // fm.power_off(&mut cp.SYST);
                board::set_power_latch(gpioa, false);
                delay::ms(&mut cp.SYST, 500);
                SCB::sys_reset();
            }
        }

        batt_tick += 1;
        if batt_tick >= 400 {
            batt_tick = 0;
            let raw = batt_adc.read_channel(BATT_ADC_CHANNEL);
            writeln!(dbg, "batt raw: {} (8-bit: {})", raw, raw >> 4).ok();
        }

        if let Some(level) = debouncer.sample(board::read_ptt(gpioa)) {
            if !level {
                transmitting = true;
                radio.enter_tx(&mut cp.SYST);
                // board::set_flashlight_led(gpiob, true);
                writeln!(dbg, "TX ON  @ {} Hz (low power)", TEST_FREQ_HZ).ok();

                let fd = radio.fd6818_mut();
                let r7d = fd.read_reg(&mut cp.SYST, 0x7D);
                let r40 = fd.read_reg(&mut cp.SYST, 0x40);
                let r30 = fd.read_reg(&mut cp.SYST, 0x30);
                let r36 = fd.read_reg(&mut cp.SYST, 0x36);
                let r19 = fd.read_reg(&mut cp.SYST, 0x19);
                writeln!(
                    dbg,
                    "regs: 0x7D={:#06x} 0x40={:#06x} 0x30={:#06x} 0x36={:#06x} 0x19={:#06x}",
                    r7d, r40, r30, r36, r19
                )
                .ok();

                let mut line1: TextBuf<24> = TextBuf::new();
                let mut line2: TextBuf<24> = TextBuf::new();
                let mut line3: TextBuf<24> = TextBuf::new();
                write!(line1, "7D:{:04x} 40:{:04x}", r7d, r40).ok();
                write!(line2, "30:{:04x} 36:{:04x}", r30, r36).ok();
                write!(line3, "19:{:04x} TX", r19).ok();

                Rectangle::new(Point::new(0, 0), Size::new(128, 64))
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                    .draw(&mut lcd)
                    .ok();
                Text::new(
                    line1.as_str(),
                    Point::new(0, 10),
                    MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
                )
                .draw(&mut lcd)
                .ok();
                Text::new(
                    line2.as_str(),
                    Point::new(0, 22),
                    MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
                )
                .draw(&mut lcd)
                .ok();
                Text::new(
                    line3.as_str(),
                    Point::new(0, 34),
                    MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
                )
                .draw(&mut lcd)
                .ok();
                lcd.flush().ok();
            } else {
                transmitting = false;
                radio.end_tx(&mut cp.SYST);
                // board::set_flashlight_led(gpiob, false);
                writeln!(dbg, "RX ON  @ {} Hz", TEST_FREQ_HZ).ok();
            }
        }

        if !transmitting {
            let audio_open = radio.poll_squelch(&mut cp.SYST, 3);

            rssi_tick += 1;
            if rssi_tick >= 40 {
                rssi_tick = 0;
                let rssi = radio.rssi(&mut cp.SYST);
                writeln!(dbg, "RSSI: {} audio_open={}", rssi, audio_open).ok();
            }
        }

        // fm_tick += 1;
        // if fm_tick >= 400 {
        //     fm_tick = 0;
        //     // let (complete, seek_failed, is_station, rssi) = fm.status(&mut cp.SYST);
        //     writeln!(
        //         dbg,
        //         "fm status: complete={} seek_failed={} is_station={} rssi={}",
        //         complete, seek_failed, is_station, rssi
        //     )
        //     .ok();
        // }

        delay::ms(&mut cp.SYST, 5);
    }
}
