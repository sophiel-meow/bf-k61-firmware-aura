use crate::hal::wear_leveled::crc16_xmodem;

pub const HEADER: u8 = 0xA6;

pub const CMD_READ: u8 = 0x52; // 'R'
pub const CMD_WRITE: u8 = 0x57; // 'W'
pub const CMD_END: u8 = 0x45; // 'E'

/// For experiments: read-only dump of the on-chip bootloader region.
/// a distinct command rather than a `CMD_READ` logical address,
/// since it addresses internal MCU program flash than every other
/// command here (the external SPI NOR flash `Storage` wraps).
pub const CMD_READ_BOOT: u8 = 0x42; // 'B'

pub const CMD_READ_LOGO: u8 = 0x4C; // 'L'
pub const CMD_WRITE_LOGO: u8 = 0x6C; // 'l'

pub const CMD_ERROR: u8 = 0xEE;

pub const ERR_BAD_CRC: u8 = 0x01;
pub const ERR_BAD_ADDR: u8 = 0x02;
pub const ERR_BAD_LEN: u8 = 0x03;

pub const MAX_DATA: usize = 96;

pub struct Frame {
    pub cmd: u8,
    pub addr: u16,
    pub data: [u8; MAX_DATA],
    pub len: usize,
}

enum State {
    Header,
    Cmd,
    AddrHi,
    AddrLo,
    LenHi,
    LenLo,
    Data,
    CrcHi,
    CrcLo,
}

/// Byte-at-a-time frame decoder fed from the USART1 RX ring buffer.
pub struct FrameReader {
    state: State,
    cmd: u8,
    addr: u16,
    len: usize,
    data: [u8; MAX_DATA],
    idx: usize,
    crc_hi: u8,
}

impl FrameReader {
    pub const fn new() -> Self {
        FrameReader {
            state: State::Header,
            cmd: 0,
            addr: 0,
            len: 0,
            data: [0; MAX_DATA],
            idx: 0,
            crc_hi: 0,
        }
    }

    fn reset(&mut self) {
        self.state = State::Header;
    }

    /// Feed one received byte. Returns `Some(Frame)` once a
    /// CRC-valid frame is complete; malformed/oversized/CRC-mismatched
    /// frames are silently dropped and the reader resyncs on the next
    /// header byte.
    pub fn feed(&mut self, byte: u8) -> Option<Frame> {
        match self.state {
            State::Header => {
                if byte == HEADER {
                    self.state = State::Cmd;
                }
            }
            State::Cmd => {
                self.cmd = byte;
                self.state = State::AddrHi;
            }
            State::AddrHi => {
                self.addr = (byte as u16) << 8;
                self.state = State::AddrLo;
            }
            State::AddrLo => {
                self.addr |= byte as u16;
                self.state = State::LenHi;
            }
            State::LenHi => {
                self.len = (byte as usize) << 8;
                self.state = State::LenLo;
            }
            State::LenLo => {
                self.len |= byte as usize;
                if self.len > MAX_DATA {
                    self.reset();
                } else if self.len == 0 {
                    self.idx = 0;
                    self.state = State::CrcHi;
                } else {
                    self.idx = 0;
                    self.state = State::Data;
                }
            }
            State::Data => {
                self.data[self.idx] = byte;
                self.idx += 1;
                if self.idx == self.len {
                    self.state = State::CrcHi;
                }
            }
            State::CrcHi => {
                self.crc_hi = byte;
                self.state = State::CrcLo;
            }
            State::CrcLo => {
                let crc_rx = ((self.crc_hi as u16) << 8) | byte as u16;
                self.reset();

                let crc_calc = frame_crc(self.cmd, self.addr, &self.data[..self.len]);
                if crc_rx == crc_calc {
                    return Some(Frame {
                        cmd: self.cmd,
                        addr: self.addr,
                        data: self.data,
                        len: self.len,
                    });
                }
            }
        }
        None
    }
}

fn frame_crc(cmd: u8, addr: u16, data: &[u8]) -> u16 {
    let mut buf = [0u8; 5 + MAX_DATA];
    buf[0] = cmd;
    buf[1..3].copy_from_slice(&addr.to_be_bytes());
    buf[3..5].copy_from_slice(&(data.len() as u16).to_be_bytes());
    buf[5..5 + data.len()].copy_from_slice(data);
    crc16_xmodem(&buf[..5 + data.len()])
}

/// Encode a response frame into `out`, returning the number of bytes
/// written. `out` must be at least `8 + data.len()` bytes.
pub fn encode(out: &mut [u8], cmd: u8, addr: u16, data: &[u8]) -> usize {
    out[0] = HEADER;
    out[1] = cmd;
    out[2..4].copy_from_slice(&addr.to_be_bytes());
    out[4..6].copy_from_slice(&(data.len() as u16).to_be_bytes());
    out[6..6 + data.len()].copy_from_slice(data);
    let crc = frame_crc(cmd, addr, data);
    out[6 + data.len()..8 + data.len()].copy_from_slice(&crc.to_be_bytes());
    8 + data.len()
}
