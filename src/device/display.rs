use crate::board;
use crate::drivers::display_spec;
use display_interface::{DataFormat, WriteOnlyDataCommand};

/// LCD electronic-volume register values for contrast levels 0-4.
pub const CONTRAST_VOLUMES: [u8; 5] = [37, 41, 45, 48, 51];

type Lcd<'a, DI> = st7565::ST7565<
    DI,
    display_spec::Sc5260Spec,
    st7565::modes::GraphicsMode<'a, 128, 8>,
    128,
    64,
    8,
>;

pub struct Display<'a, DI: WriteOnlyDataCommand> {
    lcd: Option<Lcd<'a, DI>>,
}

impl<'a, DI: WriteOnlyDataCommand> Display<'a, DI> {
    pub fn new(lcd: Lcd<'a, DI>) -> Self {
        Self { lcd: Some(lcd) }
    }

    pub fn flush(&mut self) {
        if let Some(ref mut lcd) = self.lcd {
            lcd.flush().ok();
        }
    }

    /// Apply a contrast level (0-4). The st7565 crate keeps its command
    /// interface private, so we detach the SPI interface, push the two
    /// command bytes raw, and attach it back.
    pub fn set_contrast(&mut self, level: u8) {
        let lcd = self.lcd.take().unwrap();
        let (detached, mut interface) = lcd.release_display_interface();
        interface
            .send_commands(DataFormat::U8(&[
                0x81,
                CONTRAST_VOLUMES[level.min(4) as usize],
            ]))
            .ok();
        self.lcd = Some(detached.attach_display_interface(interface));
    }

    /// Give the UI layer a `&mut` reference to the raw ST7565 for drawing.
    pub fn as_draw_target(&mut self) -> &mut Lcd<'a, DI> {
        self.lcd.as_mut().unwrap()
    }
}

pub struct Backlight {
    gpiof: &'static kd32f328_pac::gpiof::RegisterBlock,
}

impl Backlight {
    pub fn new(gpiof: &'static kd32f328_pac::gpiof::RegisterBlock) -> Self {
        Self { gpiof }
    }

    pub fn on(&self) {
        board::set_lcd_backlight(self.gpiof, true);
    }

    pub fn off(&self) {
        board::set_lcd_backlight(self.gpiof, false);
    }
}
