use super::list::{draw_list, ListSource};
use super::TextBuf;
use crate::app;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

struct ContactListSource<'a>(&'a mut app::App);

impl<'a> ListSource for ContactListSource<'a> {
    fn row_count(&mut self) -> usize {
        self.0.contacts_list_row_count()
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        self.0.contacts_list_label(index, w);
    }
}

struct ContactDetailSource<'a>(&'a app::App);

impl<'a> ListSource for ContactDetailSource<'a> {
    fn row_count(&mut self) -> usize {
        self.0.contacts_field_count()
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        self.0.contacts_field_label(index, w);
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        self.0.contacts_field_value(index, w)
    }

    fn cursor(&mut self, index: usize) -> Option<usize> {
        self.0.contacts_field_cursor(index)
    }
}

pub fn draw_contacts<D>(lcd: &mut D, app: &mut app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    if app.contacts_is_detail() {
        let mut title: TextBuf<16> = TextBuf::new();
        app.contacts_detail_title(&mut title);
        let selected = app.contacts_field_index();
        let mut source = ContactDetailSource(app);
        draw_list(lcd, title.as_str(), &mut source, selected, false);
    } else {
        let selected = app.contacts_list_selected_index();
        let mut source = ContactListSource(app);
        draw_list(lcd, "CONTACTS", &mut source, selected, false);
    }
}
