/// Fixed-width decimal digit accumulator, `N` digits total.
pub(crate) struct DigitInput<const N: usize> {
    pub(crate) digits: [u8; N],
    pub(crate) len: usize,
}

impl<const N: usize> DigitInput<N> {
    pub(crate) const fn new() -> Self {
        DigitInput {
            digits: [0; N],
            len: 0,
        }
    }
    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub(crate) fn is_full(&self) -> bool {
        self.len == N
    }
    pub(crate) fn push(&mut self, digit: u8) {
        if self.len < N {
            self.digits[self.len] = digit;
            self.len += 1;
        }
    }
    pub(crate) fn backspace(&mut self) {
        self.len = self.len.saturating_sub(1);
    }
    /// Decimal value of only the digits entered so far (no zero-padding).
    /// For a 6-digit input with len=3 and digits [4,3,9,…], returns 439.
    pub(crate) fn entered_value(&self) -> u32 {
        self.digits[..self.len]
            .iter()
            .fold(0u32, |acc, &d| acc * 10 + d as u32)
    }

    /// The full `N`-digit decimal value, untyped trailing digits as `0`.
    pub(crate) fn value(&self) -> u32 {
        let mut v: u32 = 0;
        for i in 0..N {
            let d = if i < self.len {
                self.digits[i] as u32
            } else {
                0
            };
            v = v * 10 + d;
        }
        v
    }
    /// `int_digits` is how many leading digits sit before the decimal point.
    pub(crate) fn write_display(&self, int_digits: usize, w: &mut dyn core::fmt::Write) {
        for i in 0..N {
            if i == int_digits {
                let _ = w.write_char('.');
            }
            if i < self.len {
                let _ = write!(w, "{}", self.digits[i]);
            } else {
                let _ = w.write_char('-');
            }
        }
    }
}

pub(crate) fn ddmm_to_microdeg(ddmm: u32, _is_lat: bool) -> i32 {
    let deg = ddmm / 10_000;
    let min_frac_total = ddmm % 10_000; // MM * 100 + mm
    let frac_microdeg = (min_frac_total as u64 * 100 / 6) as u32;
    (deg * 100_000 + frac_microdeg) as i32
}

