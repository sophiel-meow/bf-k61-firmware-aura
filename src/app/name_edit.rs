pub(super) const MULTITAP_TIMEOUT_TICKS: u16 = 60;

pub(super) const KEY_CHARS: [&[u8]; 10] = [
    b" ",
    b",.?1",
    b"ABCabc2",
    b"DEFdef3",
    b"GHIghi4",
    b"JKLjkl5",
    b"MNOmno6",
    b"PQRSpqrs7",
    b"TUVtuv8",
    b"WXYZwxyz9",
];

pub(crate) struct NameEdit<const N: usize> {
    pub buf: [u8; N],
    pub cursor: usize,
    pending: Option<(u8, usize)>,
    idle_ticks: u16,
}

impl<const N: usize> NameEdit<N> {
    pub const fn blank() -> Self {
        NameEdit {
            buf: [0; N],
            cursor: 0,
            pending: None,
            idle_ticks: 0,
        }
    }

    pub fn start(&mut self, initial: [u8; N]) {
        self.buf = initial;
        self.cursor = initial
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(N - 1)
            .min(N - 1);
        self.pending = None;
        self.idle_ticks = 0;
    }

    pub fn finalize_pending(&mut self) {
        if self.pending.take().is_some() {
            self.cursor = (self.cursor + 1).min(N - 1);
        }
    }

    fn max_cursor(&self) -> usize {
        self.buf
            .iter()
            .rposition(|&b| b != 0)
            .map_or(0, |p| p + 1)
            .min(N - 1)
    }

    pub fn move_cursor(&mut self, left: bool) {
        self.finalize_pending();
        self.cursor = if left {
            self.cursor.saturating_sub(1)
        } else {
            (self.cursor + 1).min(self.max_cursor())
        };
    }

    pub fn backspace(&mut self) {
        self.finalize_pending();
        if self.cursor == 0 {
            return;
        }
        for i in (self.cursor - 1)..(self.buf.len() - 1) {
            self.buf[i] = self.buf[i + 1];
        }
        *self.buf.last_mut().unwrap() = 0;
        self.cursor -= 1;
    }

    fn insert_at_cursor(&mut self, ch: u8) {
        let last = self.buf.len() - 1;
        for i in (self.cursor..last).rev() {
            self.buf[i + 1] = self.buf[i];
        }
        self.buf[self.cursor] = ch;
    }

    pub fn press_digit(&mut self, digit: u8) {
        let table = KEY_CHARS[digit as usize];
        match self.pending {
            Some((d, idx)) if d == digit => {
                let next = (idx + 1) % table.len();
                self.buf[self.cursor] = table[next];
                self.pending = Some((digit, next));
            }
            _ => {
                self.finalize_pending();
                self.insert_at_cursor(table[0]);
                self.pending = Some((digit, 0));
            }
        }
        self.idle_ticks = 0;
    }

    pub fn tick(&mut self) {
        if self.pending.is_some() {
            self.idle_ticks += 1;
            if self.idle_ticks >= MULTITAP_TIMEOUT_TICKS {
                self.finalize_pending();
            }
        }
    }
}

pub(crate) fn write_name_plain<const N: usize>(buf: &[u8; N], w: &mut dyn core::fmt::Write) {
    for &b in buf {
        let ch = if b == 0 || b == 0xFF { ' ' } else { b as char };
        let _ = w.write_char(ch);
    }
}
