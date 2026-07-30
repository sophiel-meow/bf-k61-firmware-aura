pub mod icons;
mod launcher;
mod settings;
mod standby;

use launcher::draw_app_menu;
use settings::draw_settings;
use standby::draw_standby;

use crate::app;
use crate::device::display::Display;
use display_interface::WriteOnlyDataCommand;
use embedded_graphics::mono_font::{ascii::FONT_6X10, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

// TextBuf
pub struct TextBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> TextBuf<N> {
    pub fn new() -> Self {
        TextBuf {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
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

pub fn draw<DI: WriteOnlyDataCommand>(display: &mut Display<'_, DI>, app: &app::App) {
    match app.mode() {
        app::Mode::AppMenu => draw_app_menu(display.as_draw_target(), app),
        app::Mode::Settings => draw_settings(display.as_draw_target(), app),
        _ => draw_standby(display.as_draw_target(), app),
    }
    display.flush();
}
