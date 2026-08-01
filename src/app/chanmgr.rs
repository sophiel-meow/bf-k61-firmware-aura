use super::name_edit::{write_name_plain, NameEdit};
use super::{
    channel_name_str, clamp_step, digit_value, power_from_raw, power_to_raw, subaudio_from_code,
    subaudio_from_index, subaudio_index, subaudio_to_code, wrap_step, App, DigitInput, Mode,
    MAX_CHANNEL_NUM, SUBAUDIO_MAX_INDEX,
};
use crate::device::keypad::{KeyEvent, KeyEventKind, KeyId};
use crate::device::radio::{Power, SubAudio};
use crate::flash_map;
use core::fmt::Write;

// detail-page fields
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Freq,
    Sftd,
    Offset,
    RxCts,
    TxCts,
    Power,
    Wn,
    ScanAdd,
    BusyLock,
    AniCall,
    Name,
    Save,
    Delete,
}

const BASE_FIELDS: [Field; 12] = [
    Field::Freq,
    Field::Sftd,
    Field::Offset,
    Field::RxCts,
    Field::TxCts,
    Field::Power,
    Field::Wn,
    Field::ScanAdd,
    Field::BusyLock,
    Field::AniCall,
    Field::Name,
    Field::Save,
];

fn field_slots(edit: &ChannelEdit) -> usize {
    BASE_FIELDS.len() + if edit.is_new { 0 } else { 1 }
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
            Field::Freq => "FREQ",
            Field::Sftd => "SHIFT",
            Field::Offset => "OFFSET",
            Field::RxCts => "R-CTC",
            Field::TxCts => "T-CTC",
            Field::Power => "PWR",
            Field::Wn => "W/N",
            Field::ScanAdd => "SCAN",
            Field::BusyLock => "BCL",
            Field::AniCall => "CALL",
            Field::Name => "NAME",
            Field::Save => "SAVE",
            Field::Delete => "DEL",
        }
    }

    fn is_scalar(self) -> bool {
        matches!(
            self,
            Field::Sftd
                | Field::RxCts
                | Field::TxCts
                | Field::Power
                | Field::Wn
                | Field::ScanAdd
                | Field::BusyLock
                | Field::AniCall
        )
    }
}

// channel list (List phase)
const OCCUPIED_BYTES: usize = 125;

struct ChanList {
    occupied: [u8; OCCUPIED_BYTES],
    selected: Option<u16>,
}

impl ChanList {
    const fn blank() -> Self {
        ChanList {
            occupied: [0; OCCUPIED_BYTES],
            selected: None,
        }
    }

    fn scan(app: &mut App) -> Self {
        let mut occupied = [0u8; OCCUPIED_BYTES];
        for n in 0..=MAX_CHANNEL_NUM {
            if !app.storage_mut().is_channel_empty(n) {
                occupied[n as usize / 8] |= 1 << (n % 8);
            }
        }
        let selected = (0..=MAX_CHANNEL_NUM).find(|&n| Self::bit(&occupied, n));
        ChanList { occupied, selected }
    }

    fn bit(occupied: &[u8; OCCUPIED_BYTES], n: u16) -> bool {
        occupied[n as usize / 8] & (1 << (n % 8)) != 0
    }

    fn count(&self) -> u16 {
        self.occupied.iter().map(|b| b.count_ones() as u16).sum()
    }

    fn step(&self, down: bool) -> Option<u16> {
        match self.selected {
            Some(num) if down => {
                ((num + 1)..=MAX_CHANNEL_NUM).find(|&n| Self::bit(&self.occupied, n))
            }
            Some(num) => (0..num).rev().find(|&n| Self::bit(&self.occupied, n)),
            None if down => (0..=MAX_CHANNEL_NUM).find(|&n| Self::bit(&self.occupied, n)),
            None => (0..=MAX_CHANNEL_NUM)
                .rev()
                .find(|&n| Self::bit(&self.occupied, n)),
        }
    }

    fn selected_index(&self) -> usize {
        match self.selected {
            Some(num) => (0..num).filter(|&n| Self::bit(&self.occupied, n)).count(),
            None => self.count() as usize,
        }
    }

    fn channel_at(&self, index: usize) -> Option<u16> {
        if index >= self.count() as usize {
            return None;
        }
        (0..=MAX_CHANNEL_NUM)
            .filter(|&n| Self::bit(&self.occupied, n))
            .nth(index)
    }
}

fn rescan_list(app: &mut App, prefer: Option<u16>) -> ChanList {
    let mut list = ChanList::scan(app);
    if let Some(num) = prefer {
        if ChanList::bit(&list.occupied, num) {
            list.selected = Some(num);
        }
    }
    list
}

// channel detail (Detail phase)

struct ChannelEdit {
    num: u16,
    is_new: bool,
    working: flash_map::Channel,
    sftd: u8,
    offset_hz: u32,
    field_index: usize,
    editing: bool,
    snapshot: i32,
    freq_input: DigitInput<8>,
    offset_input: DigitInput<8>,
    name_edit: NameEdit<12>,
}

fn derive_shift(ch: &flash_map::Channel) -> (u8, u32) {
    let rx = ch.rx_freq_deci_hz();
    let tx = ch.tx_freq_deci_hz();
    if tx == rx {
        (0, 0)
    } else if tx > rx {
        (1, (tx - rx) * 10)
    } else {
        (2, (rx - tx) * 10)
    }
}

fn sync_tx_freq(edit: &mut ChannelEdit) {
    let rx_deci = edit.working.rx_freq_deci_hz();
    let tx_deci = match edit.sftd {
        1 => rx_deci + edit.offset_hz / 10,
        2 => rx_deci.saturating_sub(edit.offset_hz / 10),
        _ => rx_deci,
    };
    edit.working.set_tx_freq_deci_hz(tx_deci);
}

fn default_channel(freq_hz: u32) -> flash_map::Channel {
    let mut ch = flash_map::Channel::from_bytes(&[0u8; 32]);
    let deci_hz = freq_hz / 10;
    ch.set_rx_freq_deci_hz(deci_hz);
    ch.set_tx_freq_deci_hz(deci_hz);
    ch
}

impl ChannelEdit {
    fn open(num: u16, working: flash_map::Channel, is_new: bool) -> Self {
        let (sftd, offset_hz) = derive_shift(&working);
        ChannelEdit {
            num,
            is_new,
            working,
            sftd,
            offset_hz,
            field_index: 0,
            editing: false,
            snapshot: 0,
            freq_input: DigitInput::new(),
            offset_input: DigitInput::new(),
            name_edit: NameEdit::blank(),
        }
    }

    fn is_editing(&self, index: usize) -> bool {
        self.editing && self.field_index == index
    }
}

fn current_value(edit: &ChannelEdit, field: Field) -> i32 {
    match field {
        Field::Sftd => edit.sftd as i32,
        Field::RxCts => subaudio_index(subaudio_from_code(edit.working.rx_dcs_cts_num)),
        Field::TxCts => subaudio_index(subaudio_from_code(edit.working.tx_dcs_cts_num)),
        Field::Power => power_to_raw(power_from_raw(edit.working.tx_power)) as i32,
        Field::Wn => edit.working.wide_narrow() as i32,
        Field::ScanAdd => edit.working.scan_add() as i32,
        Field::BusyLock => edit.working.busy_lock() as i32,
        Field::AniCall => edit.working.ani_target().map_or(-1, |t| t as i32),
        _ => 0,
    }
}

fn adjust(edit: &mut ChannelEdit, field: Field, up: bool) {
    let cur = current_value(edit, field);
    let new_val = match field {
        Field::Sftd => clamp_step(cur, up, 0, 2),
        Field::RxCts | Field::TxCts => wrap_step(cur, up, -1, SUBAUDIO_MAX_INDEX),
        Field::Power => clamp_step(cur, up, 0, 2),
        Field::Wn | Field::ScanAdd | Field::BusyLock => 1 - cur,
        Field::AniCall => clamp_step(cur, up, -1, flash_map::addr::CONTACT_COUNT as i32 - 1),
        _ => cur,
    };
    apply(edit, field, new_val);
}

/// Value the `0` digit key jumps a scalar field straight to.
/// `RxCts`/`TxCts`/`AniCall`'s "off"/"none" sentinel is `-1`,
/// one below the first real enumerated choice; every other scalar
/// field's floor is `0`.
fn scalar_floor(field: Field) -> i32 {
    match field {
        Field::RxCts | Field::TxCts | Field::AniCall => -1,
        _ => 0,
    }
}

fn apply(edit: &mut ChannelEdit, field: Field, v: i32) {
    match field {
        Field::Sftd => {
            edit.sftd = v as u8;
            sync_tx_freq(edit);
        }
        Field::RxCts => edit.working.rx_dcs_cts_num = subaudio_to_code(subaudio_from_index(v)),
        Field::TxCts => edit.working.tx_dcs_cts_num = subaudio_to_code(subaudio_from_index(v)),
        Field::Power => edit.working.tx_power = v as u8,
        Field::Wn => edit.working.set_wide_narrow(v != 0),
        Field::ScanAdd => edit.working.set_scan_add(v != 0),
        Field::BusyLock => edit.working.set_busy_lock(v != 0),
        Field::AniCall => edit.working.set_ani_target((v >= 0).then_some(v as u8)),
        _ => {}
    }
}

fn value_text_for(edit: &ChannelEdit, index: usize, field: Field, w: &mut dyn Write) {
    match field {
        Field::Freq => {
            let hz = edit.working.rx_freq_deci_hz() * 10;
            let _ = write!(w, "{}.{:05}", hz / 1_000_000, (hz % 1_000_000) / 10);
        }
        Field::Sftd => {
            let _ = write!(
                w,
                "{}",
                match edit.sftd {
                    1 => "+",
                    2 => "-",
                    _ => "OFF",
                }
            );
        }
        Field::Offset => {
            let hz = edit.offset_hz;
            let _ = write!(w, "{}.{:03}k", hz / 1000, hz % 1000);
        }
        Field::RxCts | Field::TxCts => {
            let sub = if field == Field::RxCts {
                subaudio_from_code(edit.working.rx_dcs_cts_num)
            } else {
                subaudio_from_code(edit.working.tx_dcs_cts_num)
            };
            match sub {
                SubAudio::None => {
                    let _ = write!(w, "OFF");
                }
                SubAudio::Ctcss(hz) => {
                    let _ = write!(w, "{}.{}Hz", hz / 10, hz % 10);
                }
                SubAudio::Dcs { code, inverted } => {
                    let _ = write!(w, "D{:03o}{}", code, if inverted { "I" } else { "N" });
                }
            }
        }
        Field::Power => {
            let _ = write!(
                w,
                "{}",
                match power_from_raw(edit.working.tx_power) {
                    Power::High => "HIGH",
                    Power::Low => "LOW",
                    Power::Mid => "MID",
                }
            );
        }
        Field::Wn => {
            let _ = write!(
                w,
                "{}",
                if current_value(edit, field) != 0 {
                    "NARROW"
                } else {
                    "WIDE"
                }
            );
        }
        Field::ScanAdd | Field::BusyLock => {
            let _ = write!(
                w,
                "{}",
                if current_value(edit, field) != 0 {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }
        Field::AniCall => match edit.working.ani_target() {
            Some(t) => {
                let _ = write!(w, "#{:02}", t + 1);
            }
            None => {
                let _ = write!(w, "NONE");
            }
        },
        Field::Name => {
            let _ = write!(w, "{}", channel_name_str(&edit.working.name));
        }
        Field::Save => {}
        Field::Delete => {
            if edit.is_editing(index) {
                let _ = write!(w, "Sure? MENU");
            }
        }
    }
}

fn commit_freq_input(edit: &mut ChannelEdit) {
    if !edit.freq_input.is_empty() {
        edit.working.set_rx_freq_deci_hz(edit.freq_input.value());
        sync_tx_freq(edit);
    }
    edit.freq_input.clear();
    edit.editing = false;
}

fn commit_offset_input(edit: &mut ChannelEdit) {
    if !edit.offset_input.is_empty() {
        edit.offset_hz = edit.offset_input.value() * 10;
        sync_tx_freq(edit);
    }
    edit.offset_input.clear();
    edit.editing = false;
}

fn dispatch_freq_edit(ev: KeyEvent, edit: &mut ChannelEdit) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(digit) = digit_value(ev.key) {
        edit.freq_input.push(digit);
        if edit.freq_input.is_full() {
            commit_freq_input(edit);
        }
        return;
    }
    match ev.key {
        KeyId::Menu => commit_freq_input(edit),
        // Backspacing on an already-empty buffer (nothing typed yet, e.g. an
        // accidental Menu press into this field) has nothing left to delete,
        // so treat it as "didn't mean to touch this" and just back out.
        KeyId::Exit if edit.freq_input.is_empty() => edit.editing = false,
        KeyId::Exit => edit.freq_input.backspace(),
        _ => {}
    }
}

fn dispatch_offset_edit(ev: KeyEvent, edit: &mut ChannelEdit) {
    if ev.kind != KeyEventKind::Single {
        return;
    }
    if let Some(digit) = digit_value(ev.key) {
        edit.offset_input.push(digit);
        if edit.offset_input.is_full() {
            commit_offset_input(edit);
        }
        return;
    }
    match ev.key {
        KeyId::Menu => commit_offset_input(edit),
        KeyId::Exit if edit.offset_input.is_empty() => edit.editing = false,
        KeyId::Exit => edit.offset_input.backspace(),
        _ => {}
    }
}

fn dispatch_name_edit(ev: KeyEvent, edit: &mut ChannelEdit) {
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

fn dispatch_list(app: &mut App, ev: KeyEvent, mut list: ChanList) {
    if !matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Repeat) {
        app.chanmgr.phase = Phase::List(list);
        return;
    }
    match ev.key {
        KeyId::Up => {
            list.selected = list.step(false);
            app.chanmgr.phase = Phase::List(list);
        }
        KeyId::Down => {
            list.selected = list.step(true);
            app.chanmgr.phase = Phase::List(list);
        }
        KeyId::Menu if ev.kind == KeyEventKind::Single => match list.selected {
            Some(num) => {
                let working = app.storage_mut().read_channel(num);
                app.chanmgr.phase = Phase::Detail(ChannelEdit::open(num, working, false));
            }
            None => match (0..=MAX_CHANNEL_NUM).find(|&n| !ChanList::bit(&list.occupied, n)) {
                Some(num) => {
                    let working = default_channel(app.master_freq_hz());
                    app.chanmgr.phase = Phase::Detail(ChannelEdit::open(num, working, true));
                }
                None => app.chanmgr.phase = Phase::List(list),
            },
        },
        KeyId::Exit if ev.kind == KeyEventKind::Single => {
            app.mode = Mode::Standby;
            app.reset_key_idle();
            app.chanmgr.phase = Phase::List(list);
        }
        _ => app.chanmgr.phase = Phase::List(list),
    }
}

fn dispatch_detail(app: &mut App, ev: KeyEvent, mut edit: ChannelEdit) {
    let field = field_at(edit.field_index);

    if edit.editing {
        match field {
            Field::Name => {
                dispatch_name_edit(ev, &mut edit);
                app.chanmgr.phase = Phase::Detail(edit);
                return;
            }
            Field::Freq => {
                dispatch_freq_edit(ev, &mut edit);
                app.chanmgr.phase = Phase::Detail(edit);
                return;
            }
            Field::Offset => {
                dispatch_offset_edit(ev, &mut edit);
                app.chanmgr.phase = Phase::Detail(edit);
                return;
            }
            _ => {}
        }
    }

    if !matches!(ev.kind, KeyEventKind::Single | KeyEventKind::Repeat) {
        app.chanmgr.phase = Phase::Detail(edit);
        return;
    }

    match ev.key {
        KeyId::Up | KeyId::Down => {
            let up = ev.key == KeyId::Up;
            if !edit.editing {
                let n = field_slots(&edit);
                edit.field_index = if up {
                    (edit.field_index + n - 1) % n
                } else {
                    (edit.field_index + 1) % n
                };
            } else if field.is_scalar() {
                adjust(&mut edit, field, up);
            }
        }
        KeyId::Digit0 if ev.kind == KeyEventKind::Single && edit.editing && field.is_scalar() => {
            apply(&mut edit, field, scalar_floor(field));
        }
        KeyId::Menu if ev.kind == KeyEventKind::Single => {
            if !edit.editing {
                match field {
                    Field::Save => {
                        app.storage_mut().write_channel_rmw(edit.num, &edit.working);
                        app.refresh_channel_display(edit.num);
                        app.chanmgr.phase = Phase::List(rescan_list(app, Some(edit.num)));
                        return;
                    }
                    Field::Delete => edit.editing = true,
                    Field::Name => {
                        edit.name_edit.start(edit.working.name);
                        edit.editing = true;
                    }
                    Field::Freq => {
                        edit.freq_input.clear();
                        edit.editing = true;
                    }
                    Field::Offset => {
                        edit.offset_input.clear();
                        edit.editing = true;
                    }
                    _ => {
                        edit.snapshot = current_value(&edit, field);
                        edit.editing = true;
                    }
                }
            } else if field == Field::Delete {
                app.storage_mut()
                    .write_channel_rmw(edit.num, &flash_map::Channel::from_bytes(&[0xFF; 32]));
                app.refresh_channel_display(edit.num);
                app.chanmgr.phase = Phase::List(rescan_list(app, None));
                return;
            } else {
                edit.editing = false;
            }
        }
        KeyId::Exit if ev.kind == KeyEventKind::Single => {
            if edit.editing {
                if field.is_scalar() {
                    let snapshot = edit.snapshot;
                    apply(&mut edit, field, snapshot);
                }
                edit.editing = false;
            } else {
                let prefer = (!edit.is_new).then_some(edit.num);
                app.chanmgr.phase = Phase::List(rescan_list(app, prefer));
                return;
            }
        }
        _ => {}
    }

    app.chanmgr.phase = Phase::Detail(edit);
}

// top-level state + entry points

enum Phase {
    List(ChanList),
    Detail(ChannelEdit),
}

pub(super) struct ChanMgrUi {
    phase: Phase,
}

impl ChanMgrUi {
    pub(super) const fn new() -> Self {
        ChanMgrUi {
            phase: Phase::List(ChanList::blank()),
        }
    }
}

pub(super) fn enter(app: &mut App) {
    app.chanmgr.phase = Phase::List(ChanList::scan(app));
    app.mode = Mode::ChanMgr;
    app.input.clear();
}

pub(super) fn dispatch(app: &mut App, ev: KeyEvent) {
    let phase = core::mem::replace(&mut app.chanmgr.phase, Phase::List(ChanList::blank()));
    match phase {
        Phase::List(list) => dispatch_list(app, ev, list),
        Phase::Detail(edit) => dispatch_detail(app, ev, edit),
    }
}

pub(super) fn poll_name_timeout(app: &mut App) {
    if app.mode != Mode::ChanMgr {
        return;
    }
    if let Phase::Detail(edit) = &mut app.chanmgr.phase {
        if edit.editing && field_at(edit.field_index) == Field::Name {
            edit.name_edit.tick();
        }
    }
}

// UI getters
pub(super) fn is_detail(app: &App) -> bool {
    matches!(app.chanmgr.phase, Phase::Detail(_))
}

pub(super) fn list_row_count(app: &App) -> usize {
    match &app.chanmgr.phase {
        Phase::List(list) => list.count() as usize + 1,
        Phase::Detail(_) => 0,
    }
}

pub(super) fn list_selected_index(app: &App) -> usize {
    match &app.chanmgr.phase {
        Phase::List(list) => list.selected_index(),
        Phase::Detail(_) => 0,
    }
}

pub(super) fn list_label(app: &mut App, index: usize, w: &mut dyn Write) {
    let channel_num = match &app.chanmgr.phase {
        Phase::List(list) => list.channel_at(index),
        Phase::Detail(_) => return,
    };
    match channel_num {
        Some(num) => {
            let ch = app.storage_mut().read_channel(num);
            let name = channel_name_str(&ch.name);
            if name.is_empty() {
                let _ = write!(w, "CH{:03}", num);
            } else {
                let _ = write!(w, "{}", name);
            }
        }
        None => {
            let _ = write!(w, "+ NEW");
        }
    }
}

pub(super) fn detail_field_count(app: &App) -> usize {
    match &app.chanmgr.phase {
        Phase::Detail(edit) => field_slots(edit),
        Phase::List(_) => 0,
    }
}

pub(super) fn detail_field_index(app: &App) -> usize {
    match &app.chanmgr.phase {
        Phase::Detail(edit) => edit.field_index,
        Phase::List(_) => 0,
    }
}

pub(super) fn detail_show_arrows(app: &App) -> bool {
    match &app.chanmgr.phase {
        Phase::Detail(edit) => edit.editing && field_at(edit.field_index).is_scalar(),
        Phase::List(_) => false,
    }
}

pub(super) fn detail_label(app: &App, index: usize, w: &mut dyn Write) {
    if matches!(app.chanmgr.phase, Phase::Detail(_)) {
        let _ = write!(w, "{}", field_at(index).label());
    }
}

pub(super) fn detail_cursor(app: &App, index: usize) -> Option<usize> {
    match &app.chanmgr.phase {
        Phase::Detail(edit) if edit.is_editing(index) && field_at(index) == Field::Name => {
            Some(edit.name_edit.cursor)
        }
        _ => None,
    }
}

pub(super) fn detail_value(app: &App, index: usize, w: &mut dyn Write) -> bool {
    if let Phase::Detail(edit) = &app.chanmgr.phase {
        let field = field_at(index);
        if edit.is_editing(index) {
            match field {
                Field::Name => {
                    write_name_plain(&edit.name_edit.buf, w);
                    return true;
                }
                Field::Freq => {
                    edit.freq_input.write_display(3, w);
                    return true;
                }
                Field::Offset => {
                    edit.offset_input.write_display(3, w);
                    return true;
                }
                _ => {}
            }
        }
        value_text_for(edit, index, field, w);
        !matches!(field, Field::Save)
    } else {
        false
    }
}

pub(super) fn detail_title(app: &App, w: &mut dyn Write) {
    if let Phase::Detail(edit) = &app.chanmgr.phase {
        if edit.is_new {
            let _ = write!(w, "NEW");
        } else {
            let _ = write!(w, "CH{:03}", edit.num);
        }
    }
}
