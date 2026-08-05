use super::tx;
use super::{SstvPage, TxMode, CQ_DETAIL_ROWS, CQ_STYLE_ROW, QSO_DETAIL_ROWS, QSO_STYLE_ROW};
use crate::app::digit_value;
use crate::app::{App, Mode};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use cortex_m::peripheral::SYST;

pub(crate) fn dispatch(app: &mut App, syst: &mut SYST, ev: KeyEvent) {
    match app.sstv.page {
        SstvPage::Main => dispatch_main(app, syst, ev),
        SstvPage::CqDetail => dispatch_cq_detail(app, syst, ev),
        SstvPage::QsoDetail => dispatch_qso_detail(app, syst, ev),
        SstvPage::Transmitting => dispatch_transmitting(app, syst, ev),
    }
}

fn dispatch_main(app: &mut App, _syst: &mut SYST, ev: KeyEvent) {
    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up | KeyId::Down => {
                let up = ev.key == KeyId::Up;
                app.sstv.field_index = if up {
                    app.sstv.field_index.saturating_sub(1)
                } else {
                    (app.sstv.field_index + 1).min(1) // 0=CQ, 1=QSO
                };
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                match app.sstv.field_index {
                    0 => {
                        // Enter CQ detail
                        app.sstv.page = SstvPage::CqDetail;
                        app.sstv.field_index = 0;
                        app.sstv.editing = false;
                    }
                    _ => {
                        // Enter QSO detail
                        // Set default M1 = "599 TNX"
                        let mut m1_buf = [0u8; 20];
                        let default = b"599 TNX";
                        let len = default.len().min(20);
                        m1_buf[..len].copy_from_slice(&default[..len]);
                        app.sstv.qso_m1.buf = m1_buf;
                        app.sstv.page = SstvPage::QsoDetail;
                        app.sstv.field_index = 0;
                        app.sstv.editing = false;
                    }
                }
            }
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                app.sstv.page = SstvPage::Main;
                app.mode = Mode::Standby;
                app.reset_key_idle();
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_cq_detail(app: &mut App, _syst: &mut SYST, ev: KeyEvent) {
    if app.sstv.editing {
        dispatch_cq_edit(app, ev);
        return;
    }

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up => {
                app.sstv.field_index = app.sstv.field_index.saturating_sub(1);
            }
            KeyId::Down => {
                app.sstv.field_index = (app.sstv.field_index + 1).min(CQ_DETAIL_ROWS - 1);
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                // Field 0 = CALL (read-only, no action)
                // Field 1 = M1 (editable)
                // Field 2 = M2 (editable)
                // Field 3 = PTT to TX (handled by main loop)
                // Field 4 = TEXT style (cycled with Up/Down while editing)
                match app.sstv.field_index {
                    1 => {
                        app.sstv.cq_m1.start(app.sstv.cq_m1.buf);
                        app.sstv.editing = true;
                    }
                    2 => {
                        app.sstv.cq_m2.start(app.sstv.cq_m2.buf);
                        app.sstv.editing = true;
                    }
                    CQ_STYLE_ROW => app.sstv.editing = true,
                    _ => {}
                }
            }
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                app.sstv.page = SstvPage::Main;
                app.sstv.field_index = 0;
                app.sstv.editing = false;
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_style_edit(app: &mut App, ev: KeyEvent) {
    match ev.key {
        KeyId::Up | KeyId::Down => app.sstv.white_text = !app.sstv.white_text,
        KeyId::Menu | KeyId::Exit => app.sstv.editing = false,
        _ => {}
    }
}

fn dispatch_cq_edit(app: &mut App, ev: KeyEvent) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if app.sstv.field_index == CQ_STYLE_ROW {
        dispatch_style_edit(app, ev);
        return;
    }
    let edit: &mut super::NameEdit<20> = match app.sstv.field_index {
        1 => &mut app.sstv.cq_m1,
        2 => &mut app.sstv.cq_m2,
        _ => {
            app.sstv.editing = false;
            return;
        }
    };

    match ev.key {
        KeyId::Menu => {
            edit.finalize_pending();
            app.sstv.editing = false;
        }
        KeyId::Exit => {
            if edit.buf.iter().all(|&b| b == 0) {
                app.sstv.editing = false;
            } else {
                edit.backspace();
            }
        }
        KeyId::Up => edit.move_cursor(true),
        KeyId::Down => edit.move_cursor(false),
        key_id => {
            if let Some(d) = digit_value(key_id) {
                edit.press_digit(d);
            }
        }
    }
}

fn dispatch_qso_detail(app: &mut App, _syst: &mut SYST, ev: KeyEvent) {
    if app.sstv.editing {
        dispatch_qso_edit(app, ev);
        return;
    }

    match ev.kind {
        KeyEventKind::Single | KeyEventKind::Repeat => match ev.key {
            KeyId::Up => {
                app.sstv.field_index = app.sstv.field_index.saturating_sub(1);
            }
            KeyId::Down => {
                app.sstv.field_index = (app.sstv.field_index + 1).min(QSO_DETAIL_ROWS - 1);
            }
            KeyId::Menu if ev.kind == KeyEventKind::Single => {
                // Field 0 = DX CALL (editable)
                // Field 1 = CALL (read-only)
                // Field 2 = M1 (editable)
                // Field 3 = M2 (editable)
                // Field 4 = TEXT style (cycled with Up/Down while editing)
                // Field 5 = PTT to TX (handled by main loop)
                match app.sstv.field_index {
                    0 => {
                        app.sstv.qso_dx_call.start(app.sstv.qso_dx_call.buf);
                        app.sstv.editing = true;
                    }
                    2 => {
                        app.sstv.qso_m1.start(app.sstv.qso_m1.buf);
                        app.sstv.editing = true;
                    }
                    3 => {
                        app.sstv.qso_m2.start(app.sstv.qso_m2.buf);
                        app.sstv.editing = true;
                    }
                    QSO_STYLE_ROW => app.sstv.editing = true,
                    _ => {}
                }
            }
            KeyId::Exit if ev.kind == KeyEventKind::Single => {
                app.sstv.page = SstvPage::Main;
                app.sstv.field_index = 0;
                app.sstv.editing = false;
            }
            _ => {}
        },
        _ => {}
    }
}

fn dispatch_qso_edit(app: &mut App, ev: KeyEvent) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if app.sstv.field_index == QSO_STYLE_ROW {
        dispatch_style_edit(app, ev);
        return;
    }

    match app.sstv.field_index {
        0 => {
            // DX CALL: NameEdit<10>
            let edit = &mut app.sstv.qso_dx_call;
            match ev.key {
                KeyId::Menu => {
                    edit.finalize_pending();
                    app.sstv.editing = false;
                }
                KeyId::Exit => {
                    if edit.buf.iter().all(|&b| b == 0) {
                        app.sstv.editing = false;
                    } else {
                        edit.backspace();
                    }
                }
                KeyId::Up => edit.move_cursor(true),
                KeyId::Down => edit.move_cursor(false),
                key_id => {
                    if let Some(d) = digit_value(key_id) {
                        edit.press_digit(d);
                    }
                }
            }
        }
        2 | 3 => {
            // M1/M2: NameEdit<20>
            let edit: &mut super::NameEdit<20> = if app.sstv.field_index == 2 {
                &mut app.sstv.qso_m1
            } else {
                &mut app.sstv.qso_m2
            };
            match ev.key {
                KeyId::Menu => {
                    edit.finalize_pending();
                    app.sstv.editing = false;
                }
                KeyId::Exit => {
                    if edit.buf.iter().all(|&b| b == 0) {
                        app.sstv.editing = false;
                    } else {
                        edit.backspace();
                    }
                }
                KeyId::Up => edit.move_cursor(true),
                KeyId::Down => edit.move_cursor(false),
                key_id => {
                    if let Some(d) = digit_value(key_id) {
                        edit.press_digit(d);
                    }
                }
            }
        }
        _ => {
            app.sstv.editing = false;
        }
    }
}

fn dispatch_transmitting(_app: &mut App, _syst: &mut SYST, ev: KeyEvent) {
    if ev.kind == KeyEventKind::Single && ev.key == KeyId::Exit {
        tx::request_abort();
    }
}

pub fn handle_ptt(app: &mut App) -> bool {
    if app.sstv.editing {
        return false;
    }
    match app.sstv.page {
        SstvPage::CqDetail => {
            queue_cq_tx(app);
            true
        }
        SstvPage::QsoDetail => {
            let dx_empty = app.sstv.qso_dx_call.buf[0] == 0;
            if !dx_empty {
                queue_qso_tx(app);
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

fn queue_cq_tx(app: &mut App) {
    app.sstv.tx_mode = Some(TxMode::Cq);
    app.sstv.page = SstvPage::Transmitting;
    app.sstv.tx_line = 0;
    app.sstv.tx_aborted = false;
    app.sstv.tx_pending = true;
}

fn queue_qso_tx(app: &mut App) {
    app.sstv.tx_mode = Some(TxMode::Qso);
    app.sstv.page = SstvPage::Transmitting;
    app.sstv.tx_line = 0;
    app.sstv.tx_aborted = false;
    app.sstv.tx_pending = true;
}
