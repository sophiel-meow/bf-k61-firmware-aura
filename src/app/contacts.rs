use super::name_edit::{write_name_plain, NameEdit};
use super::{digit_value, App, DigitInput, Mode};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::flash_map::{self, addr};
use core::fmt::Write;

fn dtmf_char(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        10..=13 => (b'A' + (v - 10)) as char,
        14 => '*',
        15 => '#',
        _ => '?',
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Id,
    Name,
    Save,
    Delete,
}

const BASE_FIELDS: [Field; 3] = [Field::Id, Field::Name, Field::Save];

fn field_slots(edit: &ContactEdit) -> usize {
    BASE_FIELDS.len() + if edit.working.is_empty() { 0 } else { 1 }
}

fn field_at(index: usize) -> Field {
    if index < BASE_FIELDS.len() {
        BASE_FIELDS[index]
    } else {
        Field::Delete
    }
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Id => "ID",
            Field::Name => "NAME",
            Field::Save => "SAVE",
            Field::Delete => "DEL",
        }
    }
}

struct ContactEdit {
    idx: u8,
    working: flash_map::Contact,
    field_index: usize,
    editing: bool,
    id_input: DigitInput<3>,
    name_edit: NameEdit<11>,
}

impl ContactEdit {
    fn open(idx: u8, working: flash_map::Contact) -> Self {
        ContactEdit {
            idx,
            working,
            field_index: 0,
            editing: false,
            id_input: DigitInput::new(),
            name_edit: NameEdit::blank(),
        }
    }

    fn is_editing(&self, index: usize) -> bool {
        self.editing && self.field_index == index
    }
}

fn commit_id_input(edit: &mut ContactEdit) {
    if !edit.id_input.is_empty() {
        let v = edit.id_input.value();
        edit.working
            .set_id([(v / 100 % 10) as u8, (v / 10 % 10) as u8, (v % 10) as u8]);
    }
    edit.id_input.clear();
    edit.editing = false;
}

fn dispatch_id_edit(ev: KeyEvent, edit: &mut ContactEdit) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(digit) = digit_value(ev.key) {
        edit.id_input.push(digit);
        if edit.id_input.is_full() {
            commit_id_input(edit);
        }
        return;
    }
    match ev.key {
        KeyId::Menu => commit_id_input(edit),
        KeyId::Exit if edit.id_input.is_empty() => edit.editing = false,
        KeyId::Exit => edit.id_input.backspace(),
        _ => {}
    }
}

fn dispatch_name_edit(ev: KeyEvent, edit: &mut ContactEdit) {
    if ev.kind == KeyEventKind::Long && ev.key == KeyId::Exit {
        edit.editing = false;
        return;
    }
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(digit) = digit_value(ev.key) {
        edit.name_edit.press_digit(digit);
        return;
    }
    match ev.key {
        KeyId::Up => edit.name_edit.move_cursor(true),
        KeyId::Down => edit.name_edit.move_cursor(false),
        KeyId::Menu => {
            edit.name_edit.finalize_pending();
            edit.working.name = edit.name_edit.buf;
            edit.editing = false;
        }
        KeyId::Exit => edit.name_edit.backspace(),
        _ => {}
    }
}

fn dispatch_list(app: &mut App, ev: KeyEvent, mut selected: u8) {
    if !matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Repeat) {
        app.contacts.phase = Phase::List { selected };
        return;
    }
    match ev.key {
        KeyId::Up => {
            selected = if selected == 0 {
                addr::CONTACT_COUNT as u8 - 1
            } else {
                selected - 1
            };
            app.contacts.phase = Phase::List { selected };
        }
        KeyId::Down => {
            selected = (selected + 1) % addr::CONTACT_COUNT as u8;
            app.contacts.phase = Phase::List { selected };
        }
        KeyId::Menu if ev.kind == KeyEventKind::Single => {
            let working = app.storage_mut().read_contact(selected);
            app.contacts.phase = Phase::Detail(ContactEdit::open(selected, working));
        }

        // asterisk key to select contect to call
        KeyId::Asterisk if ev.kind == KeyEventKind::Single => {
            let contact = app.storage_mut().read_contact(selected);
            if !contact.is_empty() {
                app.set_ani_target_override(contact.id());
                app.mode = Mode::Standby;
                app.reset_key_idle();
            }
            app.contacts.phase = Phase::List { selected };
        }
        KeyId::Exit if ev.kind == KeyEventKind::Single => {
            app.mode = Mode::Standby;
            app.reset_key_idle();
            app.contacts.phase = Phase::List { selected };
        }
        _ => app.contacts.phase = Phase::List { selected },
    }
}

fn dispatch_detail(app: &mut App, ev: KeyEvent, mut edit: ContactEdit) {
    let field = field_at(edit.field_index);

    if edit.editing {
        match field {
            Field::Id => dispatch_id_edit(ev, &mut edit),
            Field::Name => dispatch_name_edit(ev, &mut edit),
            _ => {}
        }
        app.contacts.phase = Phase::Detail(edit);
        return;
    }

    if !matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Repeat) {
        app.contacts.phase = Phase::Detail(edit);
        return;
    }

    match ev.key {
        KeyId::Up | KeyId::Down => {
            let up = ev.key == KeyId::Up;
            let n = field_slots(&edit);
            edit.field_index = if up {
                (edit.field_index + n - 1) % n
            } else {
                (edit.field_index + 1) % n
            };
        }
        KeyId::Menu if ev.kind == KeyEventKind::Single => match field {
            Field::Save => {
                app.storage_mut().write_contact(edit.idx, &edit.working);
                app.contacts.phase = Phase::List { selected: edit.idx };
                return;
            }
            Field::Delete if !edit.editing => edit.editing = true,
            Field::Delete => {
                app.storage_mut()
                    .write_contact(edit.idx, &flash_map::Contact::BLANK);
                app.contacts.phase = Phase::List { selected: edit.idx };
                return;
            }
            Field::Id => {
                edit.id_input.clear();
                edit.editing = true;
            }
            Field::Name => {
                edit.name_edit.start(edit.working.name);
                edit.editing = true;
            }
        },
        KeyId::Exit if ev.kind == KeyEventKind::Single => {
            app.contacts.phase = Phase::List { selected: edit.idx };
            return;
        }
        _ => {}
    }

    app.contacts.phase = Phase::Detail(edit);
}

enum Phase {
    List { selected: u8 },
    Detail(ContactEdit),
}

pub(super) struct ContactsUi {
    phase: Phase,
}

impl ContactsUi {
    pub(super) const fn new() -> Self {
        ContactsUi {
            phase: Phase::List { selected: 0 },
        }
    }
}

pub(super) fn enter(app: &mut App) {
    app.contacts.phase = Phase::List { selected: 0 };
    app.mode = Mode::Contacts;
    app.input.clear();
}

pub(super) fn dispatch(app: &mut App, ev: KeyEvent) {
    let phase = core::mem::replace(&mut app.contacts.phase, Phase::List { selected: 0 });
    match phase {
        Phase::List { selected } => dispatch_list(app, ev, selected),
        Phase::Detail(edit) => dispatch_detail(app, ev, edit),
    }
}

pub(super) fn poll_name_timeout(app: &mut App) {
    if app.mode != Mode::Contacts {
        return;
    }
    if let Phase::Detail(edit) = &mut app.contacts.phase {
        if edit.editing && field_at(edit.field_index) == Field::Name {
            edit.name_edit.tick();
        }
    }
}

// UI getters
pub(super) fn is_detail(app: &App) -> bool {
    matches!(app.contacts.phase, Phase::Detail(_))
}

pub(super) fn list_row_count() -> usize {
    addr::CONTACT_COUNT
}

pub(super) fn list_selected_index(app: &App) -> usize {
    match &app.contacts.phase {
        Phase::List { selected } => *selected as usize,
        Phase::Detail(_) => 0,
    }
}

pub(super) fn list_label(app: &mut App, index: usize, w: &mut dyn Write) {
    if matches!(app.contacts.phase, Phase::Detail(_)) {
        return;
    }
    let contact = app.storage_mut().read_contact(index as u8);
    if contact.is_empty() {
        let _ = write!(w, "{:02} ---", index + 1);
    } else {
        let id = contact.id();
        let _ = write!(
            w,
            "{:02} {}{}{}",
            index + 1,
            dtmf_char(id[0]),
            dtmf_char(id[1]),
            dtmf_char(id[2])
        );
        let padded = pad11(&contact.name);
        let name = super::channel_name_str(&padded);
        if !name.is_empty() {
            let _ = write!(w, " {}", name);
        }
    }
}

/// `channel_name_str` scans a 12-byte buffer; contact names are 11 bytes,
/// so pad with a trailing terminator rather than duplicating the scan.
fn pad11(name: &[u8; 11]) -> [u8; 12] {
    let mut buf = [0u8; 12];
    buf[..11].copy_from_slice(name);
    buf
}

pub(super) fn detail_field_count(app: &App) -> usize {
    match &app.contacts.phase {
        Phase::Detail(edit) => field_slots(edit),
        Phase::List { .. } => 0,
    }
}

pub(super) fn detail_field_index(app: &App) -> usize {
    match &app.contacts.phase {
        Phase::Detail(edit) => edit.field_index,
        Phase::List { .. } => 0,
    }
}

pub(super) fn detail_title(app: &App, w: &mut dyn Write) {
    if let Phase::Detail(edit) = &app.contacts.phase {
        let _ = write!(w, "CONTACT #{:02}", edit.idx + 1);
    }
}

pub(super) fn detail_label(app: &App, index: usize, w: &mut dyn Write) {
    if matches!(app.contacts.phase, Phase::Detail(_)) {
        let _ = write!(w, "{}", field_at(index).label());
    }
}

pub(super) fn detail_cursor(app: &App, index: usize) -> Option<usize> {
    match &app.contacts.phase {
        Phase::Detail(edit) if edit.is_editing(index) && field_at(index) == Field::Name => {
            Some(edit.name_edit.cursor)
        }
        _ => None,
    }
}

pub(super) fn detail_value(app: &App, index: usize, w: &mut dyn Write) -> bool {
    if let Phase::Detail(edit) = &app.contacts.phase {
        let field = field_at(index);
        if edit.is_editing(index) {
            match field {
                Field::Id => {
                    edit.id_input.write_display(3, w);
                    return true;
                }
                Field::Name => {
                    write_name_plain(&edit.name_edit.buf, w);
                    return true;
                }
                Field::Delete => {
                    let _ = write!(w, "Sure? MENU");
                    return true;
                }
                _ => {}
            }
        }
        match field {
            Field::Id => {
                let id = edit.working.id();
                let _ = write!(
                    w,
                    "{}{}{}",
                    dtmf_char(id[0]),
                    dtmf_char(id[1]),
                    dtmf_char(id[2])
                );
                true
            }
            Field::Name => {
                let _ = write!(w, "{}", super::channel_name_str(&pad11(&edit.working.name)));
                true
            }
            Field::Save => false,
            Field::Delete => false,
        }
    } else {
        false
    }
}
