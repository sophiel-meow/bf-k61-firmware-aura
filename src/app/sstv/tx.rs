//! Robot 36 Color SSTV transmission engine.
use core::sync::atomic::{AtomicBool, Ordering};

use crate::device::keypad::Keypad;
use crate::device::power::Power;
use crate::device::radio::Power as TxPower;
use crate::device::radio::Radio;
use crate::device::storage::Storage;
use crate::hal::{delay, timer};
use cortex_m::peripheral::SYST;

pub const IMAGE_W: usize = 320;
pub const IMAGE_H: usize = 240;

/// Total output lines = number of source image rows (each line carries
/// both a Y scan and a U-or-V chroma scan).
pub const TOTAL_LINES: usize = 240;

/// Fixed overhead before the scanlines start: PA/PLL settling (900ms) +
/// leader/break/leader (300+10+300ms) + VIS (300ms).
const HEADER_MS: u32 = 900 + 300 + 10 + 300 + 300;

pub const ESTIMATED_TX_MS: u32 = HEADER_MS + TOTAL_LINES as u32 * 150;

/// Chroma scan sample count (half of the luminance width).
const CHROMA_SAMPLES: usize = IMAGE_W / 2;

/// Bytes per row of the precomputed flash record: full-width Y plus
/// half-width chroma
pub const ROW_STRIDE: usize = IMAGE_W + CHROMA_SAMPLES;

const SSTV_IMAGE_SIZE: usize = ROW_STRIDE * TOTAL_LINES;
const _: () = assert!(SSTV_IMAGE_SIZE == crate::flash_map::SSTV_IMAGE_SIZE);

/// Maximum number of text lines overlaid on the image.
pub const MAX_TEXT_LINES: usize = 4;

/// Robot 36 Color VIS mode code (plain 7-bit value; `transmit_vis` adds parity).
const VIS_CODE: u8 = 8;

const SYNC_HZ: u32 = 1200;
const PORCH_HZ: u32 = 1500;
const BLACK_HZ: u32 = 1500;
const WHITE_HZ: u32 = 2300;
const LEADER_HZ: u32 = 1900;
const BREAK_HZ: u32 = 1200;
/// Separator pulse frequency identifying the V scan that follows (even rows).
const SEP_V_HZ: u32 = 1500;
/// Separator pulse frequency identifying the U scan that follows (odd rows).
const SEP_U_HZ: u32 = 2300;
/// Porch between the separator pulse and the chroma scan.
const CHROMA_PORCH_HZ: u32 = 1900;

const TICK_US: u32 = 5;

/// 275us per pixel, for both the Y scan (88ms/320) and chroma (44ms/160).
const T_PIXEL: u32 = 275 / TICK_US; // 55

/// One full line: 150.000ms.
const T_LINE: u32 = 150_000 / TICK_US; // 30000

// Absolute tick offset of each segment within a line.
const AT_PORCH: u32 = 9_000 / TICK_US; // 1800  (after 9ms sync)
const AT_Y: u32 = AT_PORCH + 3_000 / TICK_US; // 2400  (after 3ms porch)
const AT_SEP: u32 = AT_Y + IMAGE_W as u32 * T_PIXEL; // 20000 (after 88ms Y)
const AT_CPORCH: u32 = AT_SEP + 4_500 / TICK_US; // 20900 (after 4.5ms separator)
const AT_CHROMA: u32 = AT_CPORCH + 1_500 / TICK_US; // 21200 (after 1.5ms porch)

/// The chroma scan must land exactly on the end of the line, or the whole
/// absolute-offset scheme is silently wrong.
const _: () = assert!(AT_CHROMA + CHROMA_SAMPLES as u32 * T_PIXEL == T_LINE);

pub fn tone_reg_word(hz: u32) -> u16 {
    (hz * 1_032_444 / 100_000) as u16
}

/// Luminance LUT: 0->1500Hz(black), 255->2300Hz(white).
fn build_luma_lut() -> [u16; 256] {
    let mut lut = [0u16; 256];
    for (i, entry) in lut.iter_mut().enumerate() {
        let hz = BLACK_HZ + (WHITE_HZ - BLACK_HZ) * i as u32 / 255;
        *entry = tone_reg_word(hz);
    }
    lut
}

const GLYPH_W: usize = 8;
const GLYPH_H: usize = 16;
/// Blank rows between consecutive text lines.
const LINE_GAP: usize = 2;

type Glyph = [u8; GLYPH_H];

fn glyph_index(ch: u8) -> usize {
    match ch {
        b' ' => 0,
        b'0'..=b'9' => 1 + (ch - b'0') as usize,
        b'A'..=b'Z' => 11 + (ch - b'A') as usize,
        b'a'..=b'z' => 11 + (ch - b'a') as usize,
        b'/' => 37,
        b'-' => 38,
        b'.' => 39,
        b',' => 40,
        b'?' => 41,
        _ => 0,
    }
}

#[rustfmt::skip]
static FONT_8X16: [Glyph; 42] = [
    [0x00; 16], // 0: space
    // 1-10: '0'-'9'
    [0x00,0x00,0x3C,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x18,0x38,0x78,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x7E,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x06,0x06,0x0C,0x18,0x30,0x60,0x60,0x7E,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x06,0x06,0x1C,0x06,0x06,0x06,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x0C,0x1C,0x3C,0x6C,0xCC,0xCC,0xFE,0x0C,0x0C,0x0C,0x0C,0x00,0x00,0x00],
    [0x00,0x00,0x7E,0x60,0x60,0x60,0x7C,0x06,0x06,0x06,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x1C,0x30,0x60,0x60,0x7C,0x66,0x66,0x66,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x7E,0x06,0x06,0x06,0x0C,0x0C,0x18,0x18,0x30,0x30,0x30,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x66,0x3C,0x66,0x66,0x66,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x66,0x66,0x66,0x3E,0x06,0x06,0x0C,0x38,0x00,0x00,0x00],
    // 11-36: 'A'-'Z'
    [0x00,0x00,0x18,0x3C,0x66,0x66,0x66,0x7E,0x66,0x66,0x66,0x66,0x66,0x00,0x00,0x00],
    [0x00,0x00,0x7C,0x66,0x66,0x66,0x7C,0x66,0x66,0x66,0x66,0x66,0x7C,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x60,0x60,0x60,0x60,0x60,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x78,0x6C,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x6C,0x78,0x00,0x00,0x00],
    [0x00,0x00,0x7E,0x60,0x60,0x60,0x7C,0x60,0x60,0x60,0x60,0x60,0x7E,0x00,0x00,0x00],
    [0x00,0x00,0x7E,0x60,0x60,0x60,0x7C,0x60,0x60,0x60,0x60,0x60,0x60,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x60,0x60,0x6E,0x66,0x66,0x66,0x66,0x3E,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x66,0x66,0x66,0x7E,0x66,0x66,0x66,0x66,0x66,0x66,0x00,0x00,0x00],
    [0x00,0x00,0x7E,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x7E,0x00,0x00,0x00],
    [0x00,0x00,0x1E,0x0C,0x0C,0x0C,0x0C,0x0C,0x0C,0x0C,0x6C,0x6C,0x38,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x6C,0x78,0x70,0x70,0x78,0x6C,0x66,0x66,0x66,0x66,0x00,0x00,0x00],
    [0x00,0x00,0x60,0x60,0x60,0x60,0x60,0x60,0x60,0x60,0x60,0x60,0x7E,0x00,0x00,0x00],
    [0x00,0x00,0x63,0x77,0x7F,0x6B,0x6B,0x63,0x63,0x63,0x63,0x63,0x63,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x66,0x76,0x7E,0x7E,0x6E,0x66,0x66,0x66,0x66,0x66,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x7C,0x66,0x66,0x66,0x66,0x7C,0x60,0x60,0x60,0x60,0x60,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x7C,0x6E,0x3E,0x00,0x00,0x00],
    [0x00,0x00,0x7C,0x66,0x66,0x66,0x66,0x7C,0x78,0x6C,0x66,0x66,0x66,0x00,0x00,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x60,0x38,0x0C,0x06,0x06,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0xFF,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x18,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3C,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x66,0x3C,0x3C,0x18,0x00,0x00,0x00],
    [0x00,0x00,0x63,0x63,0x63,0x63,0x63,0x6B,0x6B,0x7F,0x77,0x63,0x63,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x66,0x66,0x3C,0x18,0x18,0x3C,0x66,0x66,0x66,0x66,0x00,0x00,0x00],
    [0x00,0x00,0x66,0x66,0x66,0x66,0x3C,0x18,0x18,0x18,0x18,0x18,0x18,0x00,0x00,0x00],
    [0x00,0x00,0x7E,0x06,0x06,0x0C,0x18,0x18,0x30,0x60,0x60,0x60,0x7E,0x00,0x00,0x00],
    // 37: '/'  38: '-'  39: '.'  40: ','  41: '?'
    [0x00,0x00,0x06,0x06,0x0C,0x0C,0x18,0x18,0x30,0x30,0x60,0x60,0x40,0x00,0x00,0x00],
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x7E,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00],
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x00,0x00],
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x08,0x10,0x00],
    [0x00,0x00,0x3C,0x66,0x66,0x06,0x0C,0x18,0x18,0x18,0x00,0x18,0x18,0x00,0x00,0x00],
];

/// Left edge of the glyphs
const TEXT_X: usize = 16;
const TEXT_TOP: usize = 16;

const TEXT_LINE_MAX: usize = 32;

// The block must fit the frame at its maximum extent, or `mask_fill` would
// silently clip it instead.
const _: () = assert!(TEXT_TOP + MAX_TEXT_LINES * (GLYPH_H + LINE_GAP) <= IMAGE_H);
const _: () = assert!(TEXT_X + TEXT_LINE_MAX * GLYPH_W + BAR_PAD_X <= IMAGE_W);

/// Column masks are one bit per luminance sample, MSB-first within each word
/// so bit order runs left-to-right like the pixels do.
const MASK_WORDS: usize = IMAGE_W / 32; // 10
type RowMask = [u32; MASK_WORDS];
const EMPTY_MASK: RowMask = [0; MASK_WORDS];

const INK_WHITE: u8 = 255;
const INK_BLACK: u8 = 0;

/// Cb = Cr = 128 is zero colour difference, i.e. neutral grey — so a glyph
/// at `TEXT_Y` decodes as true white rather than a bright tint of whatever
/// it was drawn over.
const NEUTRAL_C: u8 = 128;

/// Horizontal padding of the backing bar either side of the text.
const BAR_PAD_X: usize = 4;

/// OR an 8-pixel glyph row into `m` at column `x`, clipping at the right edge.
fn mask_blit8(m: &mut RowMask, x: usize, bits: u8) {
    if bits == 0 || x >= IMAGE_W {
        return;
    }
    let word = x >> 5;
    // 64-bit window over columns [word*32, word*32+64). The glyph's MSB is
    // the leftmost pixel and belongs at bit (63 - sh) of that window, so its
    // LSB lands at (56 - sh); sh <= 31 keeps the shift in range.
    let v = (bits as u64) << (56 - (x & 31));
    m[word] |= (v >> 32) as u32;
    if word + 1 < MASK_WORDS {
        m[word + 1] |= v as u32;
    }
}

/// OR the column span `[from, to)` into `m`, clipping at the right edge.
fn mask_fill(m: &mut RowMask, from: usize, to: usize) {
    for x in from..to.min(IMAGE_W) {
        m[x >> 5] |= 0x8000_0000u32 >> (x & 31);
    }
}

/// Build the glyph mask (`fg`) and backing-bar mask (`bg`) for one image row.
fn render_row_masks(text: &TextLines, row: usize, fg: &mut RowMask, bg: &mut RowMask) {
    *fg = EMPTY_MASK;
    *bg = EMPTY_MASK;
    if text.count == 0 {
        return;
    }

    // One bar spanning the whole block, including the gaps between lines,
    // and sized to the longest line, rather than one per line, so a
    // left-aligned block reads as a single panel instead of a staircase.
    let block_h = text.count * GLYPH_H + (text.count - 1) * LINE_GAP;

    // The bar runs one row above and below the glyphs, then snaps outward to
    // even rows. That snap is what keeps the pair-union chroma mask honest:
    // with both bounds even the bar covers whole chroma pairs, so `bg` is
    // identical for the two rows of any pair and the union equals the bar
    // itself. Without it the union would spill one row past the bar's edge
    // and leave a single scanline of the image desaturated but still at
    // full brightness.
    let bar_top = TEXT_TOP.saturating_sub(1) & !1;
    let bar_bot = (TEXT_TOP + block_h + 2) & !1; // exclusive
    if row < bar_top || row >= bar_bot {
        return;
    }
    let bar_w = text.max_len() * GLYPH_W;
    mask_fill(
        bg,
        TEXT_X.saturating_sub(BAR_PAD_X),
        TEXT_X + bar_w + BAR_PAD_X,
    );

    if row < TEXT_TOP {
        return; // top padding row: bar only
    }
    let off = row - TEXT_TOP;
    let glyph_row = off % (GLYPH_H + LINE_GAP);
    if off >= block_h || glyph_row >= GLYPH_H {
        return; // bottom padding, or the gap between two lines
    }

    for (ci, &ch) in text.slices()[off / (GLYPH_H + LINE_GAP)].iter().enumerate() {
        mask_blit8(
            fg,
            TEXT_X + ci * GLYPH_W,
            FONT_8X16[glyph_index(ch)][glyph_row],
        );
    }
}

struct Overlay<'a> {
    text: &'a TextLines,
    /// Which pair (`row >> 1`) the masks below describe; `usize::MAX` = none.
    pair: usize,
    fg: [RowMask; 2],
    bg: [RowMask; 2],
    /// `fg | bg` over both rows of the pair.
    chroma: RowMask,
}

impl<'a> Overlay<'a> {
    fn new(text: &'a TextLines) -> Self {
        Overlay {
            text,
            pair: usize::MAX,
            fg: [EMPTY_MASK; 2],
            bg: [EMPTY_MASK; 2],
            chroma: EMPTY_MASK,
        }
    }

    fn load_pair(&mut self, pair: usize) {
        if self.pair == pair {
            return;
        }
        let text = self.text;
        for half in 0..2 {
            render_row_masks(
                text,
                pair * 2 + half,
                &mut self.fg[half],
                &mut self.bg[half],
            );
        }
        for w in 0..MASK_WORDS {
            self.chroma[w] = self.fg[0][w] | self.bg[0][w] | self.fg[1][w] | self.bg[1][w];
        }
        self.pair = pair;
    }

    /// Composite onto one `ROW_STRIDE`-byte record in place.
    ///
    /// Rows with no text cost `MASK_WORDS` word tests and nothing else, so
    /// this disappears into the sync window on all but the ~70 text rows.
    fn apply(&mut self, row_buf: &mut [u8; ROW_STRIDE], row: usize) {
        self.load_pair(row >> 1);
        let half = row & 1;
        let (glyph_y, bar_y) = if self.text.white_text {
            (INK_WHITE, INK_BLACK)
        } else {
            (INK_BLACK, INK_WHITE)
        };

        for w in 0..MASK_WORDS {
            let (fg, bg, c) = (self.fg[half][w], self.bg[half][w], self.chroma[w]);
            if (fg | bg | c) == 0 {
                continue;
            }
            for bit in 0..32 {
                let m = 0x8000_0000u32 >> bit;
                let col = w * 32 + bit;
                if fg & m != 0 {
                    row_buf[col] = glyph_y;
                } else if bg & m != 0 {
                    row_buf[col] = bar_y;
                }
                if c & m != 0 {
                    row_buf[IMAGE_W + col / 2] = NEUTRAL_C;
                }
            }
        }
    }
}

pub struct TextLines {
    pub data: [[u8; TEXT_LINE_MAX]; MAX_TEXT_LINES],
    pub lens: [usize; MAX_TEXT_LINES],
    pub count: usize,
    /// `true` = white glyphs on a black bar, `false` = the inverse.
    pub white_text: bool,
}

impl TextLines {
    pub fn new(white_text: bool) -> Self {
        TextLines {
            data: [[0u8; TEXT_LINE_MAX]; MAX_TEXT_LINES],
            lens: [0; MAX_TEXT_LINES],
            count: 0,
            white_text,
        }
    }

    fn push(&mut self, s: &str) {
        if self.count >= MAX_TEXT_LINES || s.is_empty() {
            return;
        }
        let bytes = s.as_bytes();
        let n = bytes.len().min(TEXT_LINE_MAX);
        self.data[self.count][..n].copy_from_slice(&bytes[..n]);
        self.lens[self.count] = n;
        self.count += 1;
    }

    fn push_fmt(&mut self, prefix: &str, call: &str) {
        if self.count >= MAX_TEXT_LINES || call.is_empty() {
            return;
        }
        let mut buf = [0u8; TEXT_LINE_MAX];
        let mut len = 0;
        for &b in prefix.as_bytes() {
            if len < TEXT_LINE_MAX {
                buf[len] = b;
                len += 1;
            }
        }
        for &b in call.as_bytes() {
            if len < TEXT_LINE_MAX {
                buf[len] = b;
                len += 1;
            }
        }
        self.data[self.count] = buf;
        self.lens[self.count] = len;
        self.count += 1;
    }

    pub fn slices(&self) -> [&[u8]; MAX_TEXT_LINES] {
        let mut out: [&[u8]; MAX_TEXT_LINES] = [b""; MAX_TEXT_LINES];
        for (i, slot) in out.iter_mut().enumerate().take(self.count) {
            *slot = &self.data[i][..self.lens[i]];
        }
        out
    }

    /// Longest line, in characters — the backing bar is sized to it.
    fn max_len(&self) -> usize {
        let mut max = 0;
        for i in 0..self.count {
            if self.lens[i] > max {
                max = self.lens[i];
            }
        }
        max
    }
}

pub fn build_cq_lines(call: &str, m1: &str, m2: &str, white_text: bool) -> TextLines {
    let mut tl = TextLines::new(white_text);
    tl.push("CQ CQ");
    tl.push_fmt("DE ", call);
    tl.push(m1);
    tl.push(m2);
    tl
}

pub fn build_qso_lines(
    dx_call: &str,
    call: &str,
    m1: &str,
    m2: &str,
    white_text: bool,
) -> TextLines {
    let mut tl = TextLines::new(white_text);
    tl.push(dx_call);
    tl.push_fmt("DE ", call);
    tl.push(m1);
    tl.push(m2);
    tl
}

static SSTV_ABORT: AtomicBool = AtomicBool::new(false);

pub fn request_abort() {
    SSTV_ABORT.store(true, Ordering::SeqCst);
}

fn abort_flagged() -> bool {
    let was = SSTV_ABORT.load(Ordering::SeqCst);
    if was {
        SSTV_ABORT.store(false, Ordering::SeqCst);
    }
    was
}

/// Transmit VIS header: start bit(1200Hz/30ms) + 7-bit mode code + even-parity
/// bit (LSB first, binary1=1100Hz, binary0=1300Hz, 30ms each) + stop bit(1200Hz/30ms).
fn transmit_vis(syst: &mut SYST, radio: &mut Radio, mode_code: u8) {
    let sync = tone_reg_word(SYNC_HZ);
    let bit1_hz = tone_reg_word(1100); // binary 1
    let bit0_hz = tone_reg_word(1300); // binary 0

    let code = mode_code & 0x7F;
    let parity = (code.count_ones() & 1) as u8; // even parity over code+parity bit
    let byte = code | (parity << 7);

    radio.sstv_set_tone(syst, sync);
    delay::ms(syst, 30);
    for bit in 0..8 {
        let is_one = (byte >> bit) & 1 != 0;
        radio.sstv_set_tone(syst, if is_one { bit1_hz } else { bit0_hz });
        delay::ms(syst, 30);
    }
    radio.sstv_set_tone(syst, sync);
    delay::ms(syst, 30);
}

#[inline(always)]
fn set_tone(radio: &mut Radio, syst: &mut SYST, last: &mut u16, word: u16) {
    if word != *last {
        radio.sstv_set_tone(syst, word);
        *last = word;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn transmit_sstv(
    radio: &mut Radio,
    storage: &mut Storage,
    keypad: &mut Keypad,
    power: &Power,
    syst: &mut SYST,
    has_image: bool,
    text: &TextLines,
    tx_freq_hz: u32,
    subaudio_tx: crate::device::radio::SubAudio,
    tx_power: TxPower,
    tx_line: &mut u16,
) -> bool {
    let luma_lut = build_luma_lut();
    let sync_word = tone_reg_word(SYNC_HZ);
    let porch_word = tone_reg_word(PORCH_HZ);

    radio.enter_tx_sstv(
        syst,
        tx_freq_hz,
        subaudio_tx,
        tx_power,
        tone_reg_word(LEADER_HZ),
    );

    // delay::ms(syst, 900);

    // Header: leader(1900Hz,300ms) + break(1200Hz,10ms) + leader(1900Hz,300ms) + VIS
    radio.sstv_set_tone(syst, tone_reg_word(LEADER_HZ));
    delay::ms(syst, 300);
    radio.sstv_set_tone(syst, tone_reg_word(BREAK_HZ));
    delay::ms(syst, 10);
    radio.sstv_set_tone(syst, tone_reg_word(LEADER_HZ));
    delay::ms(syst, 300);
    transmit_vis(syst, radio, VIS_CODE);

    // Scanline loop: 240 lines, each Y scan + one chroma (V/U) scan
    // Each row is a precomputed ROW_STRIDE-byte record from flash:
    // [0..IMAGE_W) = Y, [IMAGE_W..ROW_STRIDE) = chroma (V or U, picked by
    // the upload tool per row parity), with the CQ/QSO text composited on
    // top here.
    let mut row_buf = [0u8; ROW_STRIDE];
    let mut overlay = Overlay::new(text);
    let tim6 = unsafe { timer::AfskTimer::new_sstv_line_clock(T_LINE - 1) };
    let sep_v_word = tone_reg_word(SEP_V_HZ);
    let sep_u_word = tone_reg_word(SEP_U_HZ);
    let chroma_porch_word = tone_reg_word(CHROMA_PORCH_HZ);

    // Tone currently loaded in the FD6818; `set_tone` skips redundant writes.
    let mut last_tone = u16::MAX;

    *tx_line = 0;

    // Started once, here — not per line and not per segment. From this
    // point the counter is the authority on where we are in the picture.
    tim6.sync_start();

    for src_row in 0..TOTAL_LINES {
        // t = 0: sync pulse, 1200Hz for 9ms.
        set_tone(radio, syst, &mut last_tone, sync_word);
        *tx_line = src_row as u16;

        if abort_flagged() || keypad.any_pressed(syst) || power.switch_off_raw() {
            tim6.stop();
            radio.exit_tx_sstv(syst);
            return false;
        }

        // Fetch this line's precomputed record, also inside the sync window.
        if has_image {
            let row_offset = (src_row * ROW_STRIDE) as u32;
            storage.read_sstv_chunk(row_offset, &mut row_buf);
        } else {
            for (col, px) in row_buf[..IMAGE_W].iter_mut().enumerate() {
                *px = (col * 255 / (IMAGE_W - 1)) as u8;
            }
            row_buf[IMAGE_W..].fill(128);
        }

        // Composite the text, still inside the sync window
        overlay.apply(&mut row_buf, src_row);

        // Porch: 1500Hz / 3ms
        tim6.wait_until(AT_PORCH);
        set_tone(radio, syst, &mut last_tone, porch_word);

        // Y scan: 320 samples, 275us (55 ticks) each = 88ms.
        // The tone for a sample is programmed at its own start tick, then we
        // wait for the tick that ends it
        tim6.wait_until(AT_Y);
        for col in 0..IMAGE_W {
            set_tone(radio, syst, &mut last_tone, luma_lut[row_buf[col] as usize]);
            tim6.wait_until(AT_Y + (col as u32 + 1) * T_PIXEL);
        }

        // Separator pulse (t = AT_SEP): identifies V (even row) or U (odd row).
        let sep_word = if src_row % 2 == 0 {
            sep_v_word
        } else {
            sep_u_word
        };
        set_tone(radio, syst, &mut last_tone, sep_word);

        // Porch: 1900Hz / 1.5ms
        tim6.wait_until(AT_CPORCH);
        set_tone(radio, syst, &mut last_tone, chroma_porch_word);

        // Chroma scan: 160 samples, 275us each = 44ms.
        tim6.wait_until(AT_CHROMA);
        for col in 0..CHROMA_SAMPLES {
            set_tone(
                radio,
                syst,
                &mut last_tone,
                luma_lut[row_buf[IMAGE_W + col] as usize],
            );
            let end = AT_CHROMA + (col as u32 + 1) * T_PIXEL;
            if end < T_LINE {
                tim6.wait_until(end);
            }
        }

        tim6.wait_wrap();
    }

    tim6.stop();
    radio.exit_tx_sstv(syst);
    true
}
