use crate::drivers::fd6818::Power;
use crate::drivers::norflash::{NorFlash, SECTOR_SIZE};
use crate::flash_map::{self, addr};
use crate::hal::wear_leveled::WearLeveledRegion;

/// Both VFO sides (A+B), combined into one wear-leveled record.
const VFO_REGION: WearLeveledRegion<64> = WearLeveledRegion::new(addr::VFO_INFO_ADDR, 16);

/// Global settings record.
const SETTINGS_REGION: WearLeveledRegion<25> = WearLeveledRegion::new(addr::RADIO_IMFOS_ADDR, 16);

/// FM broadcast channel list: 30 slots x u16 (little-endian deci-MHz).
const FM_PAYLOAD_LEN: usize = flash_map::FM_CHANNEL_COUNT * 2;
const FM_REGION: WearLeveledRegion<FM_PAYLOAD_LEN> = WearLeveledRegion::new(addr::FM_ADDR, 16);

pub struct Storage {
    norflash: NorFlash<'static>,
    /// Bit `n` set = the channel-table sector `n` has already been erased
    /// during the current CPS write session. Reset at session start; see
    /// `write_channel` for why per-sector (not per-record) tracking is
    /// enough.
    channel_erased_mask: u8,
}

impl Storage {
    pub fn new(norflash: NorFlash<'static>) -> Self {
        Self {
            norflash,
            channel_erased_mask: 0,
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

    // VFO
    pub fn load_vfo_raw(&mut self) -> Option<[u8; 64]> {
        VFO_REGION.load(&mut self.norflash)
    }

    pub fn save_vfo_raw(&mut self, buf: &[u8; 64]) {
        VFO_REGION.save(&mut self.norflash, buf);
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

    /// Single-channel save for on-device editing:
    /// unlike `write_channel`, which assumes the caller is about to resend
    /// every sibling record in the sector (true for a CPS session, not for a
    /// one-off keypad edit), this reads the whole 4KB sector out first so
    /// the other 127 records in it survive the erase this record's own edit
    /// requires. Deleting a channel is just calling this with
    /// `Channel::from_bytes(&[0xFF; 32])`: an all-erased record round-trips
    /// through `to_bytes()` byte-for-byte, so `is_channel_empty` sees it as
    /// blank afterward.
    pub fn write_channel_rmw(&mut self, num: u16, channel: &flash_map::Channel) {
        let addr = addr::CHAN_ADDR + num as u32 * addr::CHAN_SIZE;
        let sector_addr = (addr / SECTOR_SIZE) * SECTOR_SIZE;

        let mut buf = [0u8; SECTOR_SIZE as usize];
        self.norflash.read_bytes(sector_addr, &mut buf);

        let offset = (addr - sector_addr) as usize;
        buf[offset..offset + addr::CHAN_SIZE as usize].copy_from_slice(&channel.to_bytes());

        self.norflash.erase_sector(sector_addr);
        self.norflash.write_bytes(sector_addr, &buf);
    }

    // factory reset
    pub fn factory_reset(&mut self) {
        self.norflash.erase_sector(addr::VFO_INFO_ADDR);
        self.norflash.erase_sector(addr::RADIO_IMFOS_ADDR);
    }
}
