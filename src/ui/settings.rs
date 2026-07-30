use super::list::{draw_list, ListSource};
use crate::app;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

struct SettingsSource<'a>(&'a app::App);

impl<'a> ListSource for SettingsSource<'a> {
    fn row_count(&self) -> usize {
        self.0.settings_item_count()
    }

    fn label(&self, index: usize) -> &'static str {
        self.0.settings_label_at(index)
    }

    fn value(&self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        self.0.settings_value_at(index, w);
        true
    }
}

// draw_settings
pub fn draw_settings<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let source = SettingsSource(app);
    draw_list(
        lcd,
        "SETTINGS",
        &source,
        app.settings_index(),
        app.settings_editing(),
    );
}
