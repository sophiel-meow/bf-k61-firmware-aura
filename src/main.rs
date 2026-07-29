#![no_std]
#![no_main]

use core::fmt::Write as _;
use cortex_m::peripheral::SCB;
use cortex_m_rt::entry;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    prelude::*,
    text::Text,
};
use kd32f328_pac::Peripherals;
use panic_halt as _;

mod app;
mod board;
mod drivers;
mod flash_map;
mod hal;
mod radio;

use display_interface::{DataFormat, WriteOnlyDataCommand};
use drivers::{display_spec, fd6818, keypad, norflash};
use hal::{adc, clock, debounce, delay, hal_shim, scheduler, spi, uart};

use debounce::Debouncer;
use hal_shim::{ClosurePin, SystDelay};

const DEFAULT_FREQ_HZ: u32 = 439_500_000;

const CONTRAST_VOLUMES: [u8; 5] = [37, 41, 45, 48, 51];

/// Applies a contrast level to the display. The st7565 crate keeps its
/// command interface private, so the only way to send the electronic-volume
/// command at runtime is to detach the SPI interface, push the two command
/// bytes through it raw, and attach it back.
fn apply_lcd_contrast<DI: WriteOnlyDataCommand>(
    lcd: st7565::ST7565<
        DI,
        display_spec::Sc5260Spec,
        st7565::modes::GraphicsMode<'_, 128, 8>,
        128,
        64,
        8,
    >,
    level: u8,
) -> st7565::ST7565<DI, display_spec::Sc5260Spec, st7565::modes::GraphicsMode<'_, 128, 8>, 128, 64, 8>
{
    let (detached, mut interface) = lcd.release_display_interface();
    interface
        .send_commands(DataFormat::U8(&[
            0x81,
            CONTRAST_VOLUMES[level.min(4) as usize],
        ]))
        .ok();
    detached.attach_display_interface(interface)
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

fn draw_standby<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let master = app.master_index();
    let last_signal = app.last_signal_side();

    for i in 0..2usize {
        let y = if i == 0 { 2 } else { 34 };
        let is_master = i == master;
        let freq_hz = app.side_freq_hz(i);
        let mhz = freq_hz / 1_000_000;
        let frac = (freq_hz % 1_000_000) / 10;
        let marker = if !is_master && last_signal == Some(i) {
            "*"
        } else {
            " "
        };

        let mut line: TextBuf<20> = TextBuf::new();
        write!(
            line,
            "{}{} {:3}.{:05}",
            marker,
            if i == 0 { "A" } else { "B" },
            mhz,
            frac
        )
        .ok();

        let (bg, fg) = if is_master {
            (BinaryColor::On, BinaryColor::Off)
        } else {
            (BinaryColor::Off, BinaryColor::On)
        };

        Rectangle::new(Point::new(0, y), Size::new(128, 28))
            .into_styled(PrimitiveStyle::with_fill(bg))
            .draw(lcd)
            .ok();
        Text::new(
            line.as_str(),
            Point::new(4, y + 18),
            MonoTextStyle::new(&FONT_6X10, fg),
        )
        .draw(lcd)
        .ok();
    }
}

fn draw_menu<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    Text::new(
        app.menu_item_label(),
        Point::new(4, 14),
        MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
    )
    .draw(lcd)
    .ok();

    let mut value: TextBuf<20> = TextBuf::new();
    app.menu_value_text(&mut value);

    let editing = app.menu_editing();
    let (bg, fg) = if editing {
        (BinaryColor::On, BinaryColor::Off)
    } else {
        (BinaryColor::Off, BinaryColor::On)
    };
    Rectangle::new(Point::new(0, 30), Size::new(128, 20))
        .into_styled(PrimitiveStyle::with_fill(bg))
        .draw(lcd)
        .ok();
    Text::new(
        value.as_str(),
        Point::new(4, 44),
        MonoTextStyle::new(&FONT_6X10, fg),
    )
    .draw(lcd)
    .ok();
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
    board::init_vox_adc_pin(gpioa);
    board::init_keypad_pins(gpiob, gpioc, gpiof);
    board::init_rx_led_pin(gpioa);

    let mut batt_adc = adc::Adc::new(adc_regs);
    let keys = keypad::KeyManager::new(gpiob, gpioc, gpiof);

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

    let flash_spi = spi::SpiBus::new(spi1, spi::ClockMode::Mode3, 4);
    let norflash = norflash::NorFlash::new(flash_spi, gpioa);

    let rfic = fd6818::Fd6818::new(gpiob);
    let radio = radio::Radio::new(
        rfic,
        gpioa,
        gpiob,
        radio::ChannelConfig {
            freq_hz: DEFAULT_FREQ_HZ,
            tx_freq_hz: DEFAULT_FREQ_HZ,
            wide_band: true,
            power: fd6818::Power::Low,
            subaudio_tx: fd6818::SubAudio::None,
            subaudio_rx: fd6818::SubAudio::None,
        },
    );

    let mut app = app::App::new(
        radio,
        keys,
        norflash,
        radio::ChannelConfig {
            freq_hz: DEFAULT_FREQ_HZ,
            tx_freq_hz: DEFAULT_FREQ_HZ,
            wide_band: true,
            power: fd6818::Power::Low,
            subaudio_tx: fd6818::SubAudio::None,
            subaudio_rx: fd6818::SubAudio::Ctcss(885),
        },
        &mut cp.SYST,
    );

    app.radio.enter_rx(&mut cp.SYST);

    writeln!(
        dbg,
        "RX ON  @ {} Hz, watching RSSI...",
        app.master_freq_hz()
    )
    .ok();

    let mut scheduler = scheduler::Scheduler::new();
    let mut debouncer = Debouncer::new(board::read_ptt(gpioa));

    const BATT_ADC_CHANNEL: u8 = 1;
    const VOX_ADC_CHANNEL: u8 = 0;

    let mut audio_open = false;
    let mut applied_contrast: u8 = u8::MAX;
    let mut mic_peak: u8 = 0;

    loop {
        let due = scheduler.tick();

        app.poll_keys(&mut cp.SYST);

        if board::power_switch_off(gpioa) {
            delay::ms(&mut cp.SYST, 50);
            if board::power_switch_off(gpioa) {
                writeln!(dbg, "power switch off, shutting down").ok();
                app.radio.fd6818_mut().rf_off(&mut cp.SYST);
                board::set_power_latch(gpioa, false);
                delay::ms(&mut cp.SYST, 500);
                SCB::sys_reset();
            }
        }

        if due.every_500ms {
            let raw = batt_adc.read_channel(BATT_ADC_CHANNEL);
            writeln!(dbg, "batt raw: {} (8-bit: {})", raw, raw >> 4).ok();
        }

        if let Some(level) = debouncer.sample(board::read_ptt(gpioa)) {
            app.set_ptt(&mut cp.SYST, !level);
            if app.is_transmitting() {
                writeln!(dbg, "TX ON  @ {} Hz (low power)", app.master_freq_hz()).ok();
            } else {
                writeln!(dbg, "RX ON  @ {} Hz", app.watching_freq_hz()).ok();
            }
        }

        app.poll_tot(&mut cp.SYST);

        let mic_level = (batt_adc.read_channel(VOX_ADC_CHANNEL) >> 4) as u8;
        mic_peak = mic_peak.max(mic_level);
        app.poll_vox(&mut cp.SYST, mic_level, audio_open);

        if due.every_100ms {
            if app.vox_enabled() {
                writeln!(
                    dbg,
                    "VOX mic: {} ch0: {} (peak {})",
                    mic_level,
                    mic_peak,
                    batt_adc.read_channel(0)
                )
                .ok();
            }
            mic_peak = 0;
            app.poll_auto_lock(&mut cp.SYST, audio_open);
        }

        if due.every_50ms && applied_contrast != app.contrast() {
            lcd = apply_lcd_contrast(lcd, app.contrast());
            applied_contrast = app.contrast();
        }

        if !app.is_transmitting() {
            audio_open = app.radio.poll_squelch(&mut cp.SYST, 3);
            app.poll_dual_standby(&mut cp.SYST, app.radio.rssi_open());

            if due.every_50ms {
                let rssi = app.radio.rssi(&mut cp.SYST);
                writeln!(
                    dbg,
                    "RSSI: {} audio_open={} freq={} ch_mode={} ch_num={}",
                    rssi,
                    audio_open,
                    app.watching_freq_hz(),
                    app.watching_is_channel_mode(),
                    app.watching_channel_num()
                )
                .ok();

                if app.mode() == app::Mode::Menu {
                    draw_menu(&mut lcd, &app);
                } else {
                    draw_standby(&mut lcd, &app);
                }
                lcd.flush().ok();
            }
        }

        delay::ms(&mut cp.SYST, 10);
    }
}
