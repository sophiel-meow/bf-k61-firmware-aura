mod chanmgr;
mod contacts;
pub mod cps;
mod fm;
pub mod icons;
mod launcher;
mod list;
mod scan;
mod scanqt;
mod search;
mod settings;
mod standby;

use chanmgr::draw_chanmgr;
use contacts::draw_contacts;
use fm::draw_fm;
use launcher::draw_app_menu;
use scan::draw_scan;
use scanqt::draw_scanqt;
use search::draw_search;
use settings::draw_settings;
use standby::draw_standby;

use crate::app;
use crate::device::display::Display;
use display_interface::WriteOnlyDataCommand;

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

pub fn draw<DI: WriteOnlyDataCommand>(display: &mut Display<'_, DI>, app: &mut app::App) {
    match app.mode() {
        app::Mode::AppMenu => draw_app_menu(display.as_draw_target(), &*app),
        app::Mode::Settings => draw_settings(display.as_draw_target(), &*app),
        app::Mode::ChanMgr => draw_chanmgr(display.as_draw_target(), app),
        app::Mode::Contacts => draw_contacts(display.as_draw_target(), app),
        app::Mode::Scan => draw_scan(display.as_draw_target(), &*app),
        app::Mode::Search => draw_search(display.as_draw_target(), &*app),
        app::Mode::ScanQt => draw_scanqt(display.as_draw_target(), &*app),
        app::Mode::Fm => draw_fm(display.as_draw_target(), &*app),
        _ => draw_standby(display.as_draw_target(), &*app),
    }
    display.flush();
}
