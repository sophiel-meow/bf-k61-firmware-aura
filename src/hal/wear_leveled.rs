use crate::drivers::norflash::{NorFlash, SECTOR_SIZE};

pub(crate) fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

const CLEAR_MASK: [u8; 8] = [0xFE, 0xFC, 0xF8, 0xF0, 0xE0, 0xC0, 0x80, 0x00];

const MAX_SCAN_BYTES: usize = 32;

pub struct WearLeveledRegion<const PAYLOAD_LEN: usize> {
    base_addr: u32,
    header_size: u32,
}

impl<const PAYLOAD_LEN: usize> WearLeveledRegion<PAYLOAD_LEN> {
    const SLOT_SIZE: u32 = PAYLOAD_LEN as u32 + 2;

    pub const fn new(base_addr: u32, header_size: u32) -> Self {
        WearLeveledRegion {
            base_addr,
            header_size,
        }
    }

    fn max_slots(&self) -> u32 {
        let by_space = (SECTOR_SIZE - self.header_size) / Self::SLOT_SIZE;
        let by_header_bits = self.header_size * 8;
        by_space.min(by_header_bits)
    }

    fn scan_bytes(&self) -> usize {
        (self.max_slots() as usize).div_ceil(8)
    }

    fn slot_addr(&self, index: u32) -> u32 {
        self.base_addr + self.header_size + index * Self::SLOT_SIZE
    }

    fn find_free_slot(&self, flash: &mut NorFlash) -> u32 {
        let scan_len = self.scan_bytes();
        debug_assert!(scan_len <= MAX_SCAN_BYTES);

        let mut bitmap = [0u8; MAX_SCAN_BYTES];
        flash.read_bytes(self.base_addr, &mut bitmap[..scan_len]);

        for (i, &byte) in bitmap[..scan_len].iter().enumerate() {
            if byte != 0 {
                return i as u32 * 8 + byte.trailing_zeros();
            }
        }
        u32::MAX
    }

    fn read_slot(&self, flash: &mut NorFlash, index: u32) -> Option<[u8; PAYLOAD_LEN]> {
        let mut buf = [0u8; PAYLOAD_LEN];
        let mut crc_buf = [0u8; 2];
        let addr = self.slot_addr(index);
        flash.read_bytes(addr, &mut buf);
        flash.read_bytes(addr + PAYLOAD_LEN as u32, &mut crc_buf);

        if u16::from_le_bytes(crc_buf) == crc16_xmodem(&buf) {
            Some(buf)
        } else {
            None
        }
    }

    fn write_slot(&self, flash: &mut NorFlash, index: u32, payload: &[u8; PAYLOAD_LEN]) {
        let addr = self.slot_addr(index);
        flash.write_bytes(addr, payload);
        flash.write_bytes(
            addr + PAYLOAD_LEN as u32,
            &crc16_xmodem(payload).to_le_bytes(),
        );
    }

    pub fn load(&self, flash: &mut NorFlash) -> Option<[u8; PAYLOAD_LEN]> {
        let index = self.find_free_slot(flash);
        if index >= self.max_slots() {
            return None;
        }
        self.read_slot(flash, index)
    }

    pub fn save(&self, flash: &mut NorFlash, payload: &[u8; PAYLOAD_LEN]) {
        let index = self.find_free_slot(flash);
        let max_slots = self.max_slots();

        if index < max_slots && self.read_slot(flash, index).as_ref() == Some(payload) {
            return;
        }

        let has_room = matches!(index.checked_add(1), Some(next) if next < max_slots);
        if has_room {
            self.write_slot(flash, index + 1, payload);

            let marker_byte = index / 8;
            let bit = (index % 8) as usize;
            flash.write_bytes(self.base_addr + marker_byte, &[CLEAR_MASK[bit]]);
        } else {
            flash.erase_sector(self.base_addr);
            self.write_slot(flash, 0, payload);
        }
    }
}
