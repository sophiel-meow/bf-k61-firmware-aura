#![no_std]
#![no_main]

mod app;
mod board;
mod device;
mod drivers;
mod flash_map;
mod hal;
mod ui;

use core::fmt::Write as _;
use cortex_m_rt::entry;
use kd32f328_pac::Peripherals;
use panic_halt as _;

use device::display::{Backlight, Display};
use device::keypad::Keypad;
use device::power::Power;
use device::radio::{AniConfig, ChannelConfig, Modulation, Power as TxPower, Radio, SubAudio};
use device::storage::Storage;
use drivers::{display_spec, fd6818, norflash};
use hal::{clock, delay, hal_shim, scheduler, spi, uart};
use hal_shim::{ClosurePin, SystDelay};

const DEFAULT_FREQ_HZ: u32 = 439_500_000;

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

    // board pin init
    board::init_power_pins(gpioa);
    let power = Power::new(gpioa);
    power.latch_on();

    board::init_ptt_rxd_pin(gpioa);
    board::init_flashlight_led(gpiob);
    board::init_debug_uart_tx_pin(gpioa);
    board::init_lcd_control_pins(gpiob, gpioc);
    board::init_lcd_spi_pins(gpiob);
    board::init_lcd_backlight_pin(gpiof);
    let backlight = Backlight::new(gpiof);
    backlight.on();
    board::init_fd6818_pins(gpiob);
    board::init_norflash_pins(gpioa);
    board::init_speaker_switch_pin(gpiob);
    board::init_rx_band_pins(gpioa);
    board::init_i2c_pins(gpioa);
    board::init_battery_adc_pin(gpioa);
    board::init_vox_adc_pin(gpioa);
    board::init_keypad_pins(gpiob, gpioc, gpiof);
    board::init_rx_led_pin(gpioa);

    let mut dbg = uart::DebugUart::new(usart1, clock::SYSCLK_HZ, 115_200);
    writeln!(dbg, "bfk6-fw boot, sysclk={}Hz", clock::SYSCLK_HZ).ok();

    let mut cp = cortex_m::Peripherals::take().unwrap();

    // display
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
    let mut display = Display::new(lcd);
    writeln!(dbg, "lcd init done").ok();

    // rfic
    let mut rfic = fd6818::Fd6818::new(gpiob);
    rfic.init(&mut cp.SYST);
    writeln!(dbg, "fd6818 init done").ok();

    let flash_spi = spi::SpiBus::new(spi1, spi::ClockMode::Mode3, 4);
    let mut storage = Storage::new(norflash::NorFlash::new(flash_spi, gpioa));

    let cal_buf = storage.read_calibration();
    writeln!(dbg, "norflash cal block @0xf210: {:02x?}", cal_buf).ok();

    rfic.set_xtal_adjust(cal_buf[6]);
    writeln!(dbg, "xtal_adjust = {}", cal_buf[6]).ok();

    rfic.set_audio_calibration(cal_buf[0], cal_buf[1], cal_buf[2], cal_buf[3], cal_buf[4]);
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
        cal_buf[0], cal_buf[1], cal_buf[3], cal_buf[4],
        cal_buf[11], cal_buf[12], cal_buf[13], cal_buf[14]
    )
    .ok();

    if let Some(target) = storage.read_pa_calibration(DEFAULT_FREQ_HZ, TxPower::Low) {
        rfic.set_pa_calibration(target);
    }
    writeln!(dbg, "pa calibration loaded").ok();

    let ani_raw = storage.read_ani_raw();
    let ani = AniConfig::from_raw(
        [ani_raw[0], ani_raw[1], ani_raw[2]],
        ani_raw[9],
        ani_raw[10],
    );

    // radio class
    let radio = Radio::new(
        rfic,
        gpioa,
        gpiob,
        adc_regs,
        ChannelConfig {
            freq_hz: DEFAULT_FREQ_HZ,
            tx_freq_hz: DEFAULT_FREQ_HZ,
            wide_band: true,
            power: TxPower::Low,
            subaudio_tx: SubAudio::None,
            subaudio_rx: SubAudio::None,
            modulation: Modulation::Fm,
        },
        ani,
    );

    // app class
    let mut app = app::App::new(
        radio,
        Keypad::new(gpiob, gpioc, gpiof),
        storage,
        ChannelConfig {
            freq_hz: DEFAULT_FREQ_HZ,
            tx_freq_hz: DEFAULT_FREQ_HZ,
            wide_band: true,
            power: TxPower::Low,
            subaudio_tx: SubAudio::None,
            subaudio_rx: SubAudio::Ctcss(885),
            modulation: Modulation::Fm,
        },
        &mut cp.SYST,
    );

    app.radio_mut().enter_rx(&mut cp.SYST);

    writeln!(
        dbg,
        "RX ON  @ {} Hz, watching RSSI...",
        app.master_freq_hz()
    )
    .ok();

    // scheduler + main loop
    let mut scheduler = scheduler::Scheduler::new();
    let mut applied_contrast: u8 = u8::MAX;

    loop {
        let due = scheduler.tick();

        app.poll_keys(&mut cp.SYST);

        // power sw
        if power.debounced_off(&mut cp.SYST) {
            writeln!(dbg, "power switch off, shutting down").ok();
            app.radio_mut().rf_off(&mut cp.SYST);
            power.shutdown(&mut cp.SYST);
        }

        // battery
        if due.every_500ms {
            app.poll_battery();
            writeln!(dbg, "batt bars: {}", app.battery_bars()).ok();
        }

        // PTT
        if let Some(level) = app.radio_mut().poll_ptt() {
            app.set_ptt(&mut cp.SYST, !level);
            if app.is_transmitting() {
                writeln!(dbg, "TX ON  @ {} Hz (low power)", app.master_freq_hz()).ok();
            } else {
                writeln!(dbg, "RX ON  @ {} Hz", app.watching_freq_hz()).ok();
            }
        }

        app.poll_tot(&mut cp.SYST);

        // VOX
        let mic_level = app.radio_mut().read_mic_level();
        let audio_open = app.radio_mut().audio_is_open();
        app.poll_vox(&mut cp.SYST, mic_level, audio_open);

        if due.every_100ms {
            app.poll_auto_lock(&mut cp.SYST, audio_open);
        }

        // LCD contrast
        if due.every_50ms && applied_contrast != app.contrast() {
            display.set_contrast(app.contrast());
            applied_contrast = app.contrast();
        }

        // RX path
        if !app.is_transmitting() {
            app.poll_squelch(&mut cp.SYST, 3);
            app.poll_dual_standby(&mut cp.SYST, app.rssi_open());
            app.poll_dtmf(&mut cp.SYST);
        }

        // Screen: redraw on every_50ms regardless of TX/RX
        if due.every_50ms {
            if !app.is_transmitting() {
                let rssi = app.radio_mut().rssi(&mut cp.SYST);
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
            }

            ui::draw(&mut display, &app);
        }
        delay::ms(&mut cp.SYST, 10);
    }
}
