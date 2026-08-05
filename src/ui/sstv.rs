use super::list::{draw_list, ListSource};
use super::TextBuf;
use crate::app;
use crate::app::sstv::tx::ESTIMATED_TX_MS;
use crate::app::sstv::{SstvPage, SstvUi, CQ_STYLE_ROW, QSO_STYLE_ROW};
use core::fmt::Write as _;
use embedded_graphics::mono_font::{ascii::FONT_5X8, MonoTextStyle};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

pub fn draw_sstv<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let ui = &app.sstv;
    match ui.page {
        SstvPage::Main => draw_main(lcd, ui),
        SstvPage::CqDetail => draw_cq_detail(lcd, app),
        SstvPage::QsoDetail => draw_qso_detail(lcd, app),
        SstvPage::Transmitting => draw_transmitting(lcd, ui.tx_mode),
    }
}

struct SstvMainSource<'a> {
    _ui: &'a SstvUi,
}

impl ListSource for SstvMainSource<'_> {
    fn row_count(&mut self) -> usize {
        2
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        match index {
            0 => {
                let _ = w.write_str("TX CQ");
            }
            _ => {
                let _ = w.write_str("TX QSO");
            }
        }
    }

    fn value(&mut self, _index: usize, _w: &mut dyn core::fmt::Write) -> bool {
        false
    }
}

fn draw_main<D>(lcd: &mut D, ui: &SstvUi)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut source = SstvMainSource { _ui: ui };
    let selected = ui.field_index;
    draw_list(lcd, "SSTV", &mut source, selected, false);
}

fn style_str(ui: &SstvUi) -> &'static str {
    if ui.white_text {
        "WHITE"
    } else {
        "BLACK"
    }
}

struct CqDetailSource<'a> {
    app: &'a app::App,
    ui: &'a SstvUi,
}

impl ListSource for CqDetailSource<'_> {
    fn row_count(&mut self) -> usize {
        app::sstv::CQ_DETAIL_ROWS // CALL, M1, M2, TEXT, PTT
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        match index {
            0 => {
                let _ = w.write_str("CALL:");
            }
            1 => {
                let _ = w.write_str("M1:  ");
            }
            2 => {
                let _ = w.write_str("M2:  ");
            }
            CQ_STYLE_ROW => {
                let _ = w.write_str("TEXT:");
            }
            _ => {
                let _ = w.write_str("  PTT to TX");
            }
        }
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        match index {
            0 => {
                let call = SstvUi::aprs_call_str(self.app.settings());
                let _ = w.write_str(call);
                true
            }
            1 => {
                if self.ui.editing && self.ui.field_index == 1 {
                    super::super::app::name_edit::write_name_plain(&self.ui.cq_m1.buf, w);
                } else {
                    let s = SstvUi::name_edit_str(&self.ui.cq_m1);
                    let _ = w.write_str(if s.is_empty() { "(empty)" } else { s });
                }
                true
            }
            2 => {
                if self.ui.editing && self.ui.field_index == 2 {
                    super::super::app::name_edit::write_name_plain(&self.ui.cq_m2.buf, w);
                } else {
                    let s = SstvUi::name_edit_str(&self.ui.cq_m2);
                    let _ = w.write_str(if s.is_empty() { "(empty)" } else { s });
                }
                true
            }
            CQ_STYLE_ROW => {
                let _ = w.write_str(style_str(self.ui));
                true
            }
            _ => false,
        }
    }

    fn cursor(&mut self, index: usize) -> Option<usize> {
        if !self.ui.editing {
            return None;
        }
        match (index, self.ui.field_index) {
            (1, 1) => Some(self.ui.cq_m1.cursor),
            (2, 2) => Some(self.ui.cq_m2.cursor),
            _ => None,
        }
    }
}

fn draw_cq_detail<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let selected = app.sstv.field_index;
    let show_arrows = app.sstv.editing && selected == CQ_STYLE_ROW;
    let mut source = CqDetailSource { app, ui: &app.sstv };
    draw_list(lcd, "CQ Detail", &mut source, selected, show_arrows);
}

struct QsoDetailSource<'a> {
    app: &'a app::App,
    ui: &'a SstvUi,
}

impl ListSource for QsoDetailSource<'_> {
    fn row_count(&mut self) -> usize {
        app::sstv::QSO_DETAIL_ROWS // DX, CALL, M1, M2, TEXT, PTT
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        match index {
            0 => {
                let _ = w.write_str("DX:  ");
            }
            1 => {
                let _ = w.write_str("CALL:");
            }
            2 => {
                let _ = w.write_str("M1:  ");
            }
            3 => {
                let _ = w.write_str("M2:  ");
            }
            QSO_STYLE_ROW => {
                let _ = w.write_str("TEXT:");
            }
            _ => {
                let _ = w.write_str("  PTT to TX");
            }
        }
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        match index {
            0 => {
                if self.ui.editing && self.ui.field_index == 0 {
                    super::super::app::name_edit::write_name_plain(&self.ui.qso_dx_call.buf, w);
                } else {
                    let s = SstvUi::name_edit_str(&self.ui.qso_dx_call);
                    let _ = w.write_str(if s.is_empty() { "(required)" } else { s });
                }
                true
            }
            1 => {
                let call = SstvUi::aprs_call_str(self.app.settings());
                let _ = w.write_str(call);
                true
            }
            2 => {
                if self.ui.editing && self.ui.field_index == 2 {
                    super::super::app::name_edit::write_name_plain(&self.ui.qso_m1.buf, w);
                } else {
                    let s = SstvUi::name_edit_str(&self.ui.qso_m1);
                    let _ = w.write_str(if s.is_empty() { "(empty)" } else { s });
                }
                true
            }
            3 => {
                if self.ui.editing && self.ui.field_index == 3 {
                    super::super::app::name_edit::write_name_plain(&self.ui.qso_m2.buf, w);
                } else {
                    let s = SstvUi::name_edit_str(&self.ui.qso_m2);
                    let _ = w.write_str(if s.is_empty() { "(empty)" } else { s });
                }
                true
            }
            QSO_STYLE_ROW => {
                let _ = w.write_str(style_str(self.ui));
                true
            }
            _ => false,
        }
    }

    fn cursor(&mut self, index: usize) -> Option<usize> {
        if !self.ui.editing {
            return None;
        }
        match (index, self.ui.field_index) {
            (0, 0) => Some(self.ui.qso_dx_call.cursor),
            (2, 2) => Some(self.ui.qso_m1.cursor),
            (3, 3) => Some(self.ui.qso_m2.cursor),
            _ => None,
        }
    }
}

fn draw_qso_detail<D>(lcd: &mut D, app: &app::App)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let selected = app.sstv.field_index;
    let show_arrows = app.sstv.editing && selected == QSO_STYLE_ROW;
    let mut source = QsoDetailSource { app, ui: &app.sstv };
    draw_list(lcd, "QSO Detail", &mut source, selected, show_arrows);
}

fn draw_transmitting<D>(lcd: &mut D, tx_mode: Option<crate::app::sstv::TxMode>)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let font5 = MonoTextStyle::new(&FONT_5X8, BinaryColor::On);

    Rectangle::new(Point::new(0, 0), Size::new(128, 64))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(lcd)
        .ok();

    let title_y = 14i32;
    let mode_str = match tx_mode {
        Some(crate::app::sstv::TxMode::Cq) => "CQ",
        Some(crate::app::sstv::TxMode::Qso) => "QSO",
        None => "",
    };
    let mut title: TextBuf<16> = TextBuf::new();
    let _ = write!(title, "SSTV TX {}", mode_str);
    let title_w = title.as_str().len() as i32 * 5;
    Text::new(
        title.as_str(),
        Point::new((128 - title_w) / 2, title_y + FONT_5X8.baseline as i32),
        font5,
    )
    .draw(lcd)
    .ok();

    let wait_y = 30i32;
    let wait = "Please wait...";
    let wait_w = wait.len() as i32 * 5;
    Text::new(
        wait,
        Point::new((128 - wait_w) / 2, wait_y + FONT_5X8.baseline as i32),
        font5,
    )
    .draw(lcd)
    .ok();

    let est_secs = ESTIMATED_TX_MS.div_ceil(1000);
    let mut est_str: TextBuf<16> = TextBuf::new();
    let _ = write!(est_str, "~{}s", est_secs);
    let est_w = est_str.as_str().len() as i32 * 5;
    let est_y = 42i32;
    Text::new(
        est_str.as_str(),
        Point::new((128 - est_w) / 2, est_y + FONT_5X8.baseline as i32),
        font5,
    )
    .draw(lcd)
    .ok();

    // Hint
    let hint_y = 56i32;
    Text::new(
        "EXIT=Abort",
        Point::new(0, hint_y + FONT_5X8.baseline as i32),
        font5,
    )
    .draw(lcd)
    .ok();
}
