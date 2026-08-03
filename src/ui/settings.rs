use super::list::{draw_list, ListSource};
use crate::app;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

struct SettingsSource<'a>(&'a app::App);

impl<'a> ListSource for SettingsSource<'a> {
    fn row_count(&mut self) -> usize {
        self.0.settings_item_count()
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        let _ = write!(w, "{}", self.0.settings_label_at(index));
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        self.0.settings_value_at(index, w);
        true
    }

    fn cursor(&mut self, index: usize) -> Option<usize> {
        self.0.settings_cursor(index)
    }
}

// draw_settings
pub fn draw_settings<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut source = SettingsSource(app);
    draw_list(
        lcd,
        "SETTINGS",
        &mut source,
        app.settings_index(),
        app.settings_show_arrows(),
    );
}
