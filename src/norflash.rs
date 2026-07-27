use crate::board;
use crate::spi::SpiBus;
use kd32f328_pac::gpioa;

const CMD_WRITE_ENABLE: u8 = 0x06;
const CMD_READ_STATUS1: u8 = 0x05;
const CMD_READ_DATA: u8 = 0x03;
const CMD_PAGE_PROGRAM: u8 = 0x02;
const CMD_ERASE_SECTOR_4K: u8 = 0x20;
const CMD_ERASE_CHIP: u8 = 0xC7;

const STATUS_BUSY_BIT: u8 = 0x01;

pub const PAGE_SIZE: usize = 256;
pub const SECTOR_SIZE: u32 = 4096;

const BUSY_POLL_LIMIT: u32 = 1_000_000;

pub struct NorFlash<'a> {
    spi: SpiBus<'a>,
    gpioa: &'a gpioa::RegisterBlock,
}

impl<'a> NorFlash<'a> {
    pub fn new(spi: SpiBus<'a>, gpioa: &'a gpioa::RegisterBlock) -> Self {
        NorFlash { spi, gpioa }
    }

    fn cs_low(&mut self) {
        board::set_norflash_cs(self.gpioa, false);
    }

    fn cs_high(&mut self) {
        board::set_norflash_cs(self.gpioa, true);
    }

    fn send_addr24(&mut self, addr: u32) {
        self.spi.transfer_byte((addr >> 16) as u8);
        self.spi.transfer_byte((addr >> 8) as u8);
        self.spi.transfer_byte(addr as u8);
    }

    pub fn read_status(&mut self) -> u8 {
        self.cs_low();
        self.spi.transfer_byte(CMD_READ_STATUS1);
        let status = self.spi.transfer_byte(0x00);
        self.cs_high();
        status
    }

    fn wait_until_ready(&mut self) {
        for _ in 0..BUSY_POLL_LIMIT {
            if self.read_status() & STATUS_BUSY_BIT == 0 {
                return;
            }
        }
    }

    fn write_enable(&mut self) {
        self.cs_low();
        self.spi.transfer_byte(CMD_WRITE_ENABLE);
        self.cs_high();
    }

    pub fn read_bytes(&mut self, addr: u32, buf: &mut [u8]) {
        self.cs_low();
        self.spi.transfer_byte(CMD_READ_DATA);
        self.send_addr24(addr);
        for b in buf.iter_mut() {
            *b = self.spi.transfer_byte(0x00);
        }
        self.cs_high();
    }

    /// `addr` and `data` together must not cross a page boundary.
    fn write_page(&mut self, addr: u32, data: &[u8]) {
        self.write_enable();
        self.cs_low();
        self.spi.transfer_byte(CMD_PAGE_PROGRAM);
        self.send_addr24(addr);
        for &b in data {
            self.spi.transfer_byte(b);
        }
        self.cs_high();
        self.wait_until_ready();
    }

    pub fn write_bytes(&mut self, addr: u32, data: &[u8]) {
        let mut addr = addr;
        let mut remaining = data;
        while !remaining.is_empty() {
            let offset_in_page = (addr % PAGE_SIZE as u32) as usize;
            let chunk_len = (PAGE_SIZE - offset_in_page).min(remaining.len());
            let (chunk, rest) = remaining.split_at(chunk_len);
            self.write_page(addr, chunk);
            addr += chunk_len as u32;
            remaining = rest;
        }
    }

    /// Erases the 4KB sector containing `addr`.
    pub fn erase_sector(&mut self, addr: u32) {
        self.write_enable();
        self.cs_low();
        self.spi.transfer_byte(CMD_ERASE_SECTOR_4K);
        self.send_addr24(addr);
        self.cs_high();
        self.wait_until_ready();
    }

    pub fn erase_chip(&mut self) {
        self.write_enable();
        self.cs_low();
        self.spi.transfer_byte(CMD_ERASE_CHIP);
        self.cs_high();
        self.wait_until_ready();
    }
}
