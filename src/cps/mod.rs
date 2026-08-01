mod frame;

use cortex_m::peripheral::syst::SystClkSource;
use cortex_m::peripheral::{SCB, SYST};
use display_interface::WriteOnlyDataCommand;

use crate::app::App;
use crate::device::display::Display;
use crate::flash_map::{self, addr};
use crate::hal::clock::SYSCLK_HZ;
use crate::hal::delay;
use crate::hal::uart::{self, DebugUart};
use crate::ui;

const HANDSHAKE: &[u8] = b"PROGRAMBF-K6AURA";
const ACK: u8 = 0x06;
const IDLE_TIMEOUT_MS: u32 = 2000;

pub struct HandshakeDetector {
    matched: usize,
}

impl HandshakeDetector {
    pub const fn new() -> Self {
        HandshakeDetector { matched: 0 }
    }

    /// Drains every byte currently buffered from USART1 looking for the
    /// handshake string. Returns `true` the instant it completes.
    pub fn poll(&mut self) -> bool {
        while let Some(byte) = uart::take_byte() {
            if byte == HANDSHAKE[self.matched] {
                self.matched += 1;
                if self.matched == HANDSHAKE.len() {
                    self.matched = 0;
                    return true;
                }
            } else {
                self.matched = usize::from(byte == HANDSHAKE[0]);
            }
        }
        false
    }
}

/// Takes over the MCU for a CPS write session. Never returns: the session
/// always ends in `SCB::sys_reset()`, either on an explicit end command or
/// after 2s of silence, so the rest of `main()` never needs to unwind
/// whatever partial state a session left behind.
pub fn run_session<DI: WriteOnlyDataCommand>(
    app: &mut App,
    syst: &mut SYST,
    display: &mut Display<'_, DI>,
    dbg: &mut DebugUart,
) -> ! {
    app.radio_mut().rf_off(syst);
    app.storage_mut().reset_channel_write_session();
    dbg.write_byte(ACK);

    ui::cps::draw_programming(display.as_draw_target());
    display.flush();

    syst.set_clock_source(SystClkSource::Core);
    syst.set_reload(SYSCLK_HZ / 1000 - 1);
    syst.clear_current();
    syst.enable_counter();

    let mut reader = frame::FrameReader::new();
    let mut idle_ms: u32 = 0;

    loop {
        if syst.has_wrapped() {
            idle_ms += 1;
            if idle_ms >= IDLE_TIMEOUT_MS {
                SCB::sys_reset();
            }
        }

        let Some(byte) = uart::take_byte() else {
            continue;
        };
        idle_ms = 0;

        let Some(frame) = reader.feed(byte) else {
            continue;
        };

        match frame.cmd {
            frame::CMD_READ => handle_read(app, dbg, frame.addr),
            frame::CMD_WRITE => handle_write(app, dbg, frame.addr, &frame.data[..frame.len]),
            frame::CMD_READ_BOOT => handle_read_boot(dbg, frame.addr),
            frame::CMD_END => {
                send_response(dbg, frame::CMD_END, frame.addr, &[]);
                delay::ms(syst, 5); // let the ACK bytes drain out before reset
                SCB::sys_reset();
            }
            _ => send_error(dbg, frame.addr, frame::ERR_BAD_LEN),
        }
    }
}

/// A `READ` request never carries data, so the response length is determined
/// purely by which region `wire_addr` falls into, not by anything the request
/// claims.
fn handle_read(app: &mut App, dbg: &mut DebugUart, wire_addr: u16) {
    let logical = wire_addr as u32;
    let mut buf = [0u8; frame::MAX_DATA];

    let len = if logical < addr::VFO_INFO_ADDR {
        read_channel(app, logical, &mut buf)
    } else if logical == addr::VFO_INFO_ADDR {
        let raw = app.storage_mut().load_vfo_raw().unwrap_or([0xFF; 64]);
        buf[..64].copy_from_slice(&raw);
        Some(64)
    } else if logical == addr::RADIO_IMFOS_ADDR {
        let settings = app
            .storage_mut()
            .load_settings()
            .unwrap_or(flash_map::Settings::DEFAULT);
        // The CPS wire protocol only knows the original 19-byte settings
        // block; the trailing programmable-key bytes are this rewrite's own
        // addition and have no home on the wire, so only the prefix is sent.
        buf[..19].copy_from_slice(&settings.to_bytes()[..19]);
        Some(19)
    } else if logical == addr::FM_ADDR {
        let channels = app
            .storage_mut()
            .load_fm_channels()
            .unwrap_or([flash_map::FM_CHANNEL_EMPTY; flash_map::FM_CHANNEL_COUNT]);
        let n = flash_map::FM_CHANNEL_COUNT * 2;
        for (chunk, v) in buf[..n].chunks_exact_mut(2).zip(channels.iter()) {
            chunk.copy_from_slice(&v.to_le_bytes());
        }
        Some(n)
    } else {
        None
    };

    match len {
        Some(len) => send_response(dbg, frame::CMD_READ, wire_addr, &buf[..len]),
        None => send_error(dbg, wire_addr, frame::ERR_BAD_ADDR),
    }
}

fn read_channel(app: &mut App, logical: u32, buf: &mut [u8; frame::MAX_DATA]) -> Option<usize> {
    if !logical.is_multiple_of(addr::CHAN_SIZE) {
        return None;
    }
    let num = (logical / addr::CHAN_SIZE) as u16;
    let len = addr::CHAN_SIZE as usize;
    buf[..len].copy_from_slice(&app.storage_mut().read_channel(num).to_bytes());
    Some(len)
}

fn handle_write(app: &mut App, dbg: &mut DebugUart, wire_addr: u16, data: &[u8]) {
    let logical = wire_addr as u32;

    let ok = if logical < addr::VFO_INFO_ADDR {
        write_channel(app, logical, data)
    } else if logical == addr::VFO_INFO_ADDR && data.len() == 64 {
        let raw: [u8; 64] = data.try_into().unwrap();
        app.storage_mut().save_vfo_raw(&raw);
        true
    } else if logical == addr::RADIO_IMFOS_ADDR && data.len() == 19 {
        // Merge onto the currently-stored settings rather than a fresh
        // all-zero record, so a CPS write (which only ever sends the
        // original 19-byte block) can't silently wipe the trailing
        // programmable-key bytes it has no knowledge of.
        let current = app
            .storage_mut()
            .load_settings()
            .unwrap_or(flash_map::Settings::DEFAULT);
        let mut raw = current.to_bytes();
        raw[..19].copy_from_slice(data);
        app.storage_mut()
            .save_settings(&flash_map::Settings::from_bytes(&raw));
        true
    } else if logical == addr::FM_ADDR && data.len() == flash_map::FM_CHANNEL_COUNT * 2 {
        let mut channels = [flash_map::FM_CHANNEL_EMPTY; flash_map::FM_CHANNEL_COUNT];
        for (v, pair) in channels.iter_mut().zip(data.chunks_exact(2)) {
            *v = u16::from_le_bytes([pair[0], pair[1]]);
        }
        app.storage_mut().save_fm_channels(&channels);
        true
    } else {
        false
    };

    if ok {
        send_response(dbg, frame::CMD_WRITE, wire_addr, &[]);
    } else {
        send_error(dbg, wire_addr, frame::ERR_BAD_ADDR);
    }
}

fn write_channel(app: &mut App, logical: u32, data: &[u8]) -> bool {
    if !logical.is_multiple_of(addr::CHAN_SIZE) || data.len() != addr::CHAN_SIZE as usize {
        return false;
    }
    let num = (logical / addr::CHAN_SIZE) as u16;
    let raw: [u8; addr::CHAN_SIZE as usize] = data.try_into().unwrap();
    app.storage_mut()
        .write_channel(num, &flash_map::Channel::from_bytes(&raw));
    true
}

const BOOTLOADER_BASE: u32 = 0x0800_0000;
const BOOTLOADER_LEN: u32 = 0x2000;
const BOOTLOADER_CHUNK: u32 = 32;

fn handle_read_boot(dbg: &mut DebugUart, wire_addr: u16) {
    let offset = wire_addr as u32;
    if !offset.is_multiple_of(BOOTLOADER_CHUNK) || offset + BOOTLOADER_CHUNK > BOOTLOADER_LEN {
        send_error(dbg, wire_addr, frame::ERR_BAD_ADDR);
        return;
    }
    let mut buf = [0u8; BOOTLOADER_CHUNK as usize];
    let src = (BOOTLOADER_BASE + offset) as *const u8;
    unsafe { core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len()) };
    send_response(dbg, frame::CMD_READ_BOOT, wire_addr, &buf);
}

fn send_response(dbg: &mut DebugUart, cmd: u8, addr: u16, data: &[u8]) {
    let mut out = [0u8; 8 + frame::MAX_DATA];
    let n = frame::encode(&mut out, cmd, addr, data);
    dbg.write_bytes(&out[..n]);
}

fn send_error(dbg: &mut DebugUart, addr: u16, code: u8) {
    send_response(dbg, frame::CMD_ERROR, addr, &[code]);
}
