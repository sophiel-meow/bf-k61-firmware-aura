pub mod keys;
pub mod tx;

use super::name_edit::NameEdit;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SstvPage {
    Main,
    CqDetail,
    QsoDetail,
    Transmitting,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TxMode {
    Cq,
    Qso,
}

pub const CQ_DETAIL_ROWS: usize = 5;
pub const QSO_DETAIL_ROWS: usize = 6;

pub const CQ_STYLE_ROW: usize = CQ_DETAIL_ROWS - 2;
pub const QSO_STYLE_ROW: usize = QSO_DETAIL_ROWS - 2;

pub struct SstvUi {
    pub page: SstvPage,
    /// Whether an image is stored in SPI flash.
    pub has_image: bool,

    // CQ one-shot fields
    pub cq_m1: NameEdit<20>,
    pub cq_m2: NameEdit<20>,

    // QSO one-shot fields
    pub qso_dx_call: NameEdit<10>,
    pub qso_m1: NameEdit<20>,
    pub qso_m2: NameEdit<20>,

    pub white_text: bool,

    // TX state
    pub tx_line: u16,
    pub tx_aborted: bool,
    pub tx_mode: Option<TxMode>,

    /// Currently selected row in detail pages.
    pub field_index: usize,
    /// Whether currently editing a text field.
    pub editing: bool,
    /// True when a TX has been queued (handled by main loop).
    pub tx_pending: bool,
}

impl SstvUi {
    pub const fn new() -> Self {
        SstvUi {
            page: SstvPage::Main,
            has_image: false,
            cq_m1: NameEdit::blank(),
            cq_m2: NameEdit::blank(),
            qso_dx_call: NameEdit::blank(),
            qso_m1: NameEdit::blank(),
            qso_m2: NameEdit::blank(),
            white_text: true,
            tx_line: 0,
            tx_aborted: false,
            tx_mode: None,
            field_index: 0,
            editing: false,
            tx_pending: false,
        }
    }

    pub fn name_edit_str<const N: usize>(edit: &NameEdit<N>) -> &str {
        let buf = &edit.buf;
        let end = buf.iter().position(|&b| b == 0).unwrap_or(N);
        core::str::from_utf8(&buf[..end]).unwrap_or("")
    }

    pub fn aprs_call_str(settings: &crate::flash_map::Settings) -> &str {
        let buf = &settings.aprs_callsign;
        let end = buf.iter().position(|&b| b == 0).unwrap_or(7);
        core::str::from_utf8(&buf[..end]).unwrap_or("")
    }
}

pub fn poll_name_timeout(ui: &mut SstvUi) {
    if ui.editing {
        match ui.page {
            SstvPage::CqDetail => match ui.field_index {
                1 => ui.cq_m1.tick(),
                2 => ui.cq_m2.tick(),
                _ => {}
            },
            SstvPage::QsoDetail => match ui.field_index {
                0 => ui.qso_dx_call.tick(),
                2 => ui.qso_m1.tick(),
                3 => ui.qso_m2.tick(),
                _ => {}
            },
            _ => {}
        }
    }
}
