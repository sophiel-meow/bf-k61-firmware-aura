pub const FLAG: u8 = 0x7E;

pub const MAX_FRAME: usize = 400;
pub const MAX_NRZI_BYTES: usize = 512;

// AX.25 transmits bits LSB-first within each byte, so the FCS is computed
// with the reversed polynomial (0x8408) processing bits starting from the
// LSB

const CRC_POLY_REV: u16 = 0x8408; // 0x1021 bit-reversed

fn crc16_update(mut crc: u16, byte: u8) -> u16 {
    crc ^= byte as u16;
    for _ in 0..8 {
        if crc & 0x0001 != 0 {
            crc = (crc >> 1) ^ CRC_POLY_REV;
        } else {
            crc >>= 1;
        }
    }
    crc
}

pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc = crc16_update(crc, b);
    }
    crc ^ 0xFFFF
}

pub fn encode_address(out: &mut [u8; 7], call: &[u8; 6], ssid: u8, last: bool) {
    for i in 0..6 {
        let c = call[i];
        out[i] = if c.is_ascii_uppercase() || c.is_ascii_digit() {
            c << 1
        } else {
            b' ' << 1 // space = 0x40
        };
    }
    // SSID byte: [7]=C/H, [6:5]=11b (reserved), [4:1]=SSID, [0]=extension
    // Extension bit: 0 = more addresses follow, 1 = last address
    let ssid_bits = (ssid & 0x0F) << 1;
    let last_bit = if last { 0x01 } else { 0x00 };
    out[6] = last_bit | 0x60 | ssid_bits;
}

pub struct Ax25Builder {
    buf: [u8; MAX_FRAME],
    len: usize,
}

impl Ax25Builder {
    pub fn new() -> Self {
        Ax25Builder {
            buf: [0u8; MAX_FRAME],
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len < MAX_FRAME {
            self.buf[self.len] = byte;
            self.len += 1;
        }
    }

    fn push_slice(&mut self, data: &[u8]) {
        for &b in data {
            self.push(b);
        }
    }

    /// [dest…][src…][digis…][ctrl][pid][info…][fcs_lo][fcs_hi]
    pub fn build_ui_frame(
        &mut self,
        dest_call: &[u8; 6],
        dest_ssid: u8,
        src_call: &[u8; 6],
        src_ssid: u8,
        digi_path: &[([u8; 6], u8)], // (call, ssid) pairs
        info: &[u8],
    ) -> u16 {
        self.len = 0;

        let mut addr_buf = [0u8; 7];
        encode_address(&mut addr_buf, dest_call, dest_ssid, false);
        self.push_slice(&addr_buf);

        let src_last = digi_path.is_empty();
        encode_address(&mut addr_buf, src_call, src_ssid, src_last);
        self.push_slice(&addr_buf);

        for (i, &(call, ssid)) in digi_path.iter().enumerate() {
            let last = i == digi_path.len() - 1;
            encode_address(&mut addr_buf, &call, ssid, last);
            self.push_slice(&addr_buf);
        }

        self.push(0x03); // UI frame
        self.push(0xF0); // no layer 3

        self.push_slice(info);

        let fcs = crc16_ccitt(&self.buf[..self.len]);
        self.push((fcs & 0xFF) as u8);
        self.push((fcs >> 8) as u8);

        fcs
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

pub fn nrzi_encode_frame(raw: &[u8], num_tail_flags: usize, out: &mut [u8]) -> usize {
    let mut bit_pos: usize = 0;
    let mut nrzi: u8 = 1; // start at mark (idle)

    let mut emit = |nrzi_bit: u8| {
        if bit_pos / 8 < out.len() {
            if nrzi_bit != 0 {
                out[bit_pos / 8] |= 1 << (7 - (bit_pos % 8));
            }
        }
        bit_pos += 1;
    };

    let mut ones: u8 = 0; // consecutive input 1s since last stuff/reset

    let mut process_byte = |byte: u8, stuff: bool, nrzi: &mut u8, ones: &mut u8| {
        // LSB
        for shift in 0..8 {
            let bit = (byte >> shift) & 1;

            if bit == 1 {
                *ones += 1;
            } else {
                *ones = 0;
            }

            if bit == 0 {
                *nrzi ^= 1;
            }
            emit(*nrzi);

            if stuff && *ones == 5 {
                *nrzi ^= 1;
                emit(*nrzi);
                *ones = 0;
            }
        }
    };

    let lead_flags: usize = 10;
    for _ in 0..lead_flags {
        let mut o = ones;
        let mut n = nrzi;
        process_byte(FLAG, false, &mut n, &mut o);
        nrzi = n;
        ones = 0;
    }

    let mut o = 0u8;
    for &byte in raw {
        process_byte(byte, true, &mut nrzi, &mut o);
    }

    ones = o;

    for _ in 0..num_tail_flags.max(1) {
        let mut o = ones;
        let mut n = nrzi;
        process_byte(FLAG, false, &mut n, &mut o);
        nrzi = n;
        ones = 0;
    }

    bit_pos
}
