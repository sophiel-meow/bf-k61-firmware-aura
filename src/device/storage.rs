use crate::drivers::fd6818::Power;
use crate::drivers::norflash::{NorFlash, PAGE_SIZE, SECTOR_SIZE};
use crate::flash_map::{self, addr, SatRecord, MAX_SATELLITES, SAT_RECORD_SIZE, SSTV_IMAGE_SIZE};
use crate::hal::wear_leveled::WearLeveledRegion;

/// Both VFO sides (A+B), combined into one wear-leveled record.
const VFO_REGION: WearLeveledRegion<64> = WearLeveledRegion::new(addr::VFO_INFO_ADDR, 16);

/// Global settings record.
const SETTINGS_REGION: WearLeveledRegion<{ flash_map::SETTINGS_BYTES }> =
    WearLeveledRegion::new(addr::RADIO_IMFOS_ADDR, 16);

/// FM broadcast channel list: 30 slots x u16 (little-endian deci-MHz).
const FM_PAYLOAD_LEN: usize = flash_map::FM_CHANNEL_COUNT * 2;
const FM_REGION: WearLeveledRegion<FM_PAYLOAD_LEN> = WearLeveledRegion::new(addr::FM_ADDR, 16);

/// Per-side last active VFO/Channel mode + selected channel number +
/// modulation: bytes `half*3..half*3+3` are 1 mode byte + little-endian
/// `u16` channel number, bytes `6+half` are the side's `Modulation` raw
const CHANNEL_STATE_REGION: WearLeveledRegion<8> = WearLeveledRegion::new(addr::SYSTEMRAN_ADDR, 16);

/// Satellite records: 20 * 32 bytes = 640 bytes.
const SAT_TOTAL_BYTES: usize = MAX_SATELLITES * SAT_RECORD_SIZE;

pub struct Storage {
    pub(crate) norflash: NorFlash<'static>,
    /// Bit `n` set = the channel-table sector `n` has already been erased
    /// during the current CPS write session. Reset at session start; see
    /// `write_channel` for why per-sector (not per-record) tracking is
    /// enough.
    channel_erased_mask: u8,
    /// True once the satellite sector has been erased during the current
    /// CPS write session. All 20 satellite slots share a single sector.
    sat_erased: bool,
}

impl Storage {
    pub fn new(norflash: NorFlash<'static>) -> Self {
        Self {
            norflash,
            channel_erased_mask: 0,
            sat_erased: false,
        }
    }

    // settings
    pub fn load_settings(&mut self) -> Option<flash_map::Settings> {
        SETTINGS_REGION
            .load(&mut self.norflash)
            .map(|b| flash_map::Settings::from_bytes(&b))
    }

    pub fn save_settings(&mut self, settings: &flash_map::Settings) {
        SETTINGS_REGION.save(&mut self.norflash, &settings.to_bytes());
    }

    pub fn read_boot_logo(&mut self) -> [u8; flash_map::BOOT_LOGO_SIZE] {
        let mut buf = [0u8; flash_map::BOOT_LOGO_SIZE];
        self.norflash.read_bytes(addr::BOOT_LOGO_ADDR, &mut buf);
        buf
    }

    pub fn read_boot_logo_chunk(&mut self, offset: u32, buf: &mut [u8]) {
        self.norflash.read_bytes(addr::BOOT_LOGO_ADDR + offset, buf);
    }

    pub fn erase_boot_logo(&mut self) {
        self.norflash.erase_sector(addr::BOOT_LOGO_ADDR);
    }

    pub fn write_boot_logo_chunk(&mut self, offset: u32, data: &[u8]) {
        self.norflash
            .write_bytes(addr::BOOT_LOGO_ADDR + offset, data);
    }

    // VFO
    pub fn load_vfo_raw(&mut self) -> Option<[u8; 64]> {
        VFO_REGION.load(&mut self.norflash)
    }

    pub fn save_vfo_raw(&mut self, buf: &[u8; 64]) {
        VFO_REGION.save(&mut self.norflash, buf);
    }

    // channel/VFO mode state
    pub fn load_channel_state(&mut self) -> Option<[u8; 8]> {
        CHANNEL_STATE_REGION.load(&mut self.norflash)
    }

    pub fn save_channel_state(&mut self, buf: &[u8; 8]) {
        CHANNEL_STATE_REGION.save(&mut self.norflash, buf);
    }

    // calibration

    /// Read the 16-byte factory calibration block at `0xF210`.
    pub fn read_calibration(&mut self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        self.norflash.read_bytes(0xF210, &mut buf);
        buf
    }

    /// Read a single-byte PA calibration value from the given address.
    fn read_pa_byte(&mut self, addr: u32) -> u8 {
        let mut buf = [0u8; 1];
        self.norflash.read_bytes(addr, &mut buf);
        buf[0]
    }

    /// Flash address holding the calibrated APC target byte for `freq_hz`
    /// at the given power level.
    /// Only U_400/V_136/V_200 bands are actually calibrated.
    fn pa_calibration_addr(freq_hz: u32, power: Power) -> Option<u32> {
        let base = match power {
            Power::High => addr::PA_TABLE_BASE_HIGH,
            Power::Mid => addr::PA_TABLE_BASE_MID,
            Power::Low => addr::PA_TABLE_BASE_LOW,
        };
        let mhz = freq_hz / 1_000_000;
        if freq_hz >= 400_000_000 {
            Some(base + (mhz - 400) / 10)
        } else if freq_hz >= 200_000_000 {
            Some(base + 0x20 + (mhz - 200) / 5)
        } else if freq_hz >= 130_000_000 {
            let idx = ((mhz - 130) / 3).min(15);
            Some(base + 0x10 + idx)
        } else {
            None
        }
    }

    /// Raw 11-byte `STF_DTMFSTORE` block at `0xA000`: `machine_id[5]`,
    /// `alarm_word`, `flags`, `on_time`, `off_time`, `separator`, `group_call`.
    pub fn read_ani_raw(&mut self) -> [u8; 11] {
        let mut buf = [0u8; 11];
        self.norflash.read_bytes(addr::DTMFINFOR_ADDR, &mut buf);
        buf
    }

    /// 7-byte battery voltage threshold table at `0xF200`:
    /// ascending 8-bit ADC thresholds `[low, level0, level1, level2, level3,
    /// level4, high]`. Blank flash (0x00/0xFF sentinel) falls back to the
    /// built-in defaults.
    pub fn read_battery_calibration(&mut self) -> [u8; 7] {
        let mut buf = [0u8; 7];
        self.norflash.read_bytes(addr::DEV_BATT_ADDR, &mut buf);
        if buf[0] == 0x00 || buf[0] == 0xFF {
            buf = [130, 139, 147, 156, 165, 183, 0];
        }
        buf
    }

    /// Calibrated APC target byte for the given frequency+power, read
    /// straight from flash. Returns `None` if the band isn't calibrated;
    /// the caller decides what to do with the raw value (push it into the
    /// RFIC driver, etc.) — `Storage` has no knowledge of the RFIC itself.
    pub fn read_pa_calibration(&mut self, freq_hz: u32, power: Power) -> Option<u8> {
        let addr = Self::pa_calibration_addr(freq_hz, power)?;
        Some(self.read_pa_byte(addr))
    }

    // FM broadcast channel list
    pub fn load_fm_channels(&mut self) -> Option<[u16; flash_map::FM_CHANNEL_COUNT]> {
        let buf = FM_REGION.load(&mut self.norflash)?;
        let mut channels = [flash_map::FM_CHANNEL_EMPTY; flash_map::FM_CHANNEL_COUNT];
        for (slot, pair) in channels.iter_mut().zip(buf.chunks_exact(2)) {
            *slot = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Some(channels)
    }

    pub fn save_fm_channels(&mut self, channels: &[u16; flash_map::FM_CHANNEL_COUNT]) {
        let mut buf = [0u8; FM_PAYLOAD_LEN];
        for (pair, &v) in buf.chunks_exact_mut(2).zip(channels.iter()) {
            pair.copy_from_slice(&v.to_le_bytes());
        }
        FM_REGION.save(&mut self.norflash, &buf);
    }

    // channels
    pub fn read_channel(&mut self, num: u16) -> flash_map::Channel {
        let addr = addr::CHAN_ADDR + num as u32 * addr::CHAN_SIZE;
        let mut buf = [0u8; addr::CHAN_SIZE as usize];
        self.norflash.read_bytes(addr, &mut buf);
        flash_map::Channel::from_bytes(&buf)
    }

    pub fn is_channel_empty(&mut self, num: u16) -> bool {
        let addr = addr::CHAN_ADDR + num as u32 * addr::CHAN_SIZE;
        let mut buf = [0u8; addr::CHAN_SIZE as usize];
        self.norflash.read_bytes(addr, &mut buf);
        buf.iter().all(|&b| b == 0xFF)
    }

    /// Call once at the start of a CPS write session, before any
    /// `write_channel` calls, so each touched sector gets erased exactly
    /// once for the session.
    pub fn reset_channel_write_session(&mut self) {
        self.channel_erased_mask = 0;
    }

    pub fn write_channel(&mut self, num: u16, channel: &flash_map::Channel) {
        let addr = addr::CHAN_ADDR + num as u32 * addr::CHAN_SIZE;
        let sector = addr / SECTOR_SIZE;
        let bit = 1u8 << sector;

        if self.channel_erased_mask & bit == 0 {
            self.norflash.erase_sector(sector * SECTOR_SIZE);
            self.channel_erased_mask |= bit;
        }

        self.norflash.write_bytes(addr, &channel.to_bytes());
    }

    fn rmw_sector_record(&mut self, record_addr: u32, new_record: &[u8]) {
        let sector_addr = (record_addr / SECTOR_SIZE) * SECTOR_SIZE;
        let record_off = (record_addr - sector_addr) as usize;
        let record_end = record_off + new_record.len();

        // Stage 1: sector -> scratch, patching in the new record on the fly.
        self.norflash.erase_sector(addr::RMW_SCRATCH_ADDR);
        let mut page = [0u8; PAGE_SIZE];
        let mut off = 0usize;
        while off < SECTOR_SIZE as usize {
            self.norflash
                .read_bytes(sector_addr + off as u32, &mut page);
            let page_end = off + PAGE_SIZE;
            if record_off < page_end && record_end > off {
                let lo = record_off.max(off);
                let hi = record_end.min(page_end);
                page[lo - off..hi - off]
                    .copy_from_slice(&new_record[lo - record_off..hi - record_off]);
            }
            self.norflash
                .write_bytes(addr::RMW_SCRATCH_ADDR + off as u32, &page);
            off += PAGE_SIZE;
        }

        // Stage 2: erase the real sector, copy the staged (patched) content back.
        self.norflash.erase_sector(sector_addr);
        let mut off = 0usize;
        while off < SECTOR_SIZE as usize {
            self.norflash
                .read_bytes(addr::RMW_SCRATCH_ADDR + off as u32, &mut page);
            self.norflash.write_bytes(sector_addr + off as u32, &page);
            off += PAGE_SIZE;
        }
    }

    /// Single-channel save for on-device editing:
    /// unlike `write_channel`, which assumes the caller is about to resend
    /// every sibling record in the sector (true for a CPS session, not for a
    /// one-off keypad edit), this preserves the other 127 records in the
    /// sector across the erase this record's own edit requires. Deleting a
    /// channel is just calling this with `Channel::from_bytes(&[0xFF; 32])`:
    /// an all-erased record round-trips through `to_bytes()` byte-for-byte,
    /// so `is_channel_empty` sees it as blank afterward.
    pub fn write_channel_rmw(&mut self, num: u16, channel: &flash_map::Channel) {
        let addr = addr::CHAN_ADDR + num as u32 * addr::CHAN_SIZE;
        self.rmw_sector_record(addr, &channel.to_bytes());
    }

    pub fn read_contact(&mut self, idx: u8) -> flash_map::Contact {
        let addr = addr::DTMF_CODE_ADDR + idx as u32 * addr::CONTACT_SIZE;
        let mut buf = [0u8; addr::CONTACT_SIZE as usize];
        self.norflash.read_bytes(addr, &mut buf);
        flash_map::Contact::from_bytes(&buf)
    }

    pub fn write_contact(&mut self, idx: u8, contact: &flash_map::Contact) {
        let addr = addr::DTMF_CODE_ADDR + idx as u32 * addr::CONTACT_SIZE;
        self.rmw_sector_record(addr, &contact.to_bytes());
    }

    /// Check whether an SSTV background image is stored in SPI flash.
    /// The sentinel byte at `SSTV_IMAGE_ADDR + SSTV_IMAGE_SIZE` is 0x00
    /// when an image has been loaded, or 0xFF (erased) when not.
    pub fn has_sstv_image(&mut self) -> bool {
        let mut sentinel = [0xFFu8; 1];
        let addr = addr::SSTV_IMAGE_ADDR + SSTV_IMAGE_SIZE as u32;
        self.norflash.read_bytes(addr, &mut sentinel);
        sentinel[0] == 0x00
    }

    fn erase_sstv_sectors(&mut self) {
        let start_sector = (addr::SSTV_IMAGE_ADDR / SECTOR_SIZE) * SECTOR_SIZE;
        let end = addr::SSTV_IMAGE_ADDR + SSTV_IMAGE_SIZE as u32;
        let end_sector = ((end + SECTOR_SIZE) / SECTOR_SIZE) * SECTOR_SIZE;
        for sector in (start_sector..end_sector).step_by(SECTOR_SIZE as usize) {
            self.norflash.erase_sector(sector);
        }
    }

    /// Erase the SSTV image sector and set the sentinel.
    pub fn erase_sstv_image(&mut self) {
        self.erase_sstv_sectors();
        // Write sentinel: 0x00 = "image present"
        let sentinel = [0x00u8; 1];
        self.norflash
            .write_bytes(addr::SSTV_IMAGE_ADDR + SSTV_IMAGE_SIZE as u32, &sentinel);
    }

    pub fn write_sstv_chunk(&mut self, offset: u32, data: &[u8]) {
        self.norflash
            .write_bytes(addr::SSTV_IMAGE_ADDR + offset, data);
    }

    pub fn read_sstv_chunk(&mut self, offset: u32, buf: &mut [u8]) {
        self.norflash
            .read_bytes(addr::SSTV_IMAGE_ADDR + offset, buf);
    }

    // factory reset
    pub fn factory_reset(&mut self) {
        self.norflash.erase_sector(addr::VFO_INFO_ADDR);
        self.norflash.erase_sector(addr::RADIO_IMFOS_ADDR);
        self.norflash.erase_sector(addr::SYSTEMRAN_ADDR);
    }

    /// True once `first_boot_format` has run on this device
    pub fn is_first_boot_done(&mut self) -> bool {
        let mut buf = [0u8; flash_map::FIRST_BOOT_MAGIC.len()];
        self.norflash
            .read_bytes(addr::FIRST_BOOT_MARKER_ADDR, &mut buf);
        buf == flash_map::FIRST_BOOT_MAGIC
    }

    pub fn mark_first_boot_done(&mut self) {
        self.norflash.erase_sector(addr::FIRST_BOOT_MARKER_ADDR);
        self.norflash
            .write_bytes(addr::FIRST_BOOT_MARKER_ADDR, &flash_map::FIRST_BOOT_MAGIC);
    }

    /// One-time cleanup for a device that may still carry non-erased
    /// factory data in flash regions our own record parsers only treat as
    /// "blank" when literally all-`0xFF` (voice-prompt audio at `SAT_ADDR`,
    /// DTMF codes, etc. Deliberately leaves the channel table alone: its byte
    /// layout was match the factory's
    pub fn first_boot_format(&mut self) {
        self.factory_reset(); // VFO, settings, channel-state
        self.norflash.erase_sector(addr::DTMFINFOR_ADDR); // ANI id + DTMF contacts
        self.norflash.erase_sector(addr::FM_ADDR);
        self.norflash.erase_sector(addr::SAT_ADDR);
        self.norflash.erase_sector(addr::BOOT_LOGO_ADDR);
        self.erase_sstv_sectors();
    }

    pub fn read_raw(&mut self, addr: u32, buf: &mut [u8]) {
        self.norflash.read_bytes(addr, buf);
    }

    pub fn load_satellites(&mut self) -> [Option<SatRecord>; MAX_SATELLITES] {
        let mut buf = [0xFFu8; SAT_TOTAL_BYTES];
        self.norflash.read_bytes(addr::SAT_ADDR, &mut buf);

        let mut sats = [None; MAX_SATELLITES];
        for (i, chunk) in buf.chunks_exact(SAT_RECORD_SIZE).enumerate() {
            let record: [u8; SAT_RECORD_SIZE] = chunk.try_into().unwrap();
            sats[i] = SatRecord::from_bytes(&record);
        }
        sats
    }

    /// `SAT_ADDR`'s 4KB sector holds only satellite records (nothing else
    /// in `flash_map` maps into it), so unlike `write_channel_rmw`/
    /// `write_contact` there's no sibling data to preserve across the
    /// erase. Erase once, then write only the defined slots directly --
    /// deliberately avoiding a `[u8; SECTOR_SIZE]` stack buffer here: this
    /// runs deep in the keypress interrupt call chain (satellite detail
    /// save), and that 4KB on top of `main()`'s already-large frame
    /// overflowed the 17.7KB RAM budget into the bootloader's IRQ
    /// trampoline table just below RAM start, freezing the device.
    pub fn save_satellites(&mut self, sats: &[Option<SatRecord>; MAX_SATELLITES]) {
        let sector_addr = (addr::SAT_ADDR / SECTOR_SIZE) * SECTOR_SIZE;
        self.norflash.erase_sector(sector_addr);
        for (i, sat_opt) in sats.iter().enumerate() {
            if let Some(sat) = sat_opt {
                let record_addr = addr::SAT_ADDR + i as u32 * SAT_RECORD_SIZE as u32;
                self.norflash.write_bytes(record_addr, &sat.to_bytes());
            }
        }
    }

    /// Call once at the start of a CPS write session, before any
    /// `write_satellite` calls, so the satellite sector gets erased
    /// exactly once for the session.
    pub fn reset_sat_write_session(&mut self) {
        self.sat_erased = false;
    }

    /// Write a single satellite record. The first call in a CPS session
    /// erases the satellite sector; subsequent calls only write their
    /// 32-byte slot. The caller MUST write all 20 slots (empty slots as
    /// `[0xFF; 32]`) to ensure the other 19 records don't stay in an
    /// indeterminate state, erased-flash bytes (0xFF) decode as `None`,
    /// which is the correct "empty slot" representation.
    pub fn write_satellite(&mut self, idx: usize, record: &[u8; SAT_RECORD_SIZE]) {
        let record_addr = addr::SAT_ADDR + idx as u32 * SAT_RECORD_SIZE as u32;

        if !self.sat_erased {
            let sector_addr = (record_addr / SECTOR_SIZE) * SECTOR_SIZE;
            self.norflash.erase_sector(sector_addr);
            self.sat_erased = true;
        }

        self.norflash.write_bytes(record_addr, record);
    }
}
