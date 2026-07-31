use super::list::{draw_list, ListSource};
use crate::app;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

struct LauncherSource<'a>(&'a app::App);

impl<'a> ListSource for LauncherSource<'a> {
    fn row_count(&mut self) -> usize {
        self.0.launcher_item_count()
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        let _ = write!(w, "{}", self.0.launcher_label_at(index));
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        if self.0.launcher_available_at(index) {
            false
        } else {
            let _ = write!(w, "----");
            true
        }
    }
}

// draw_app_menu (launcher)
pub fn draw_app_menu<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut source = LauncherSource(app);
    draw_list(lcd, "MENU", &mut source, app.launcher_index(), false);
}
