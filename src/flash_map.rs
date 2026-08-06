pub mod addr {
    pub const CHAN_ADDR: u32 = 0x0000;
    pub const CHAN_SIZE: u32 = 32;
    // pub const NAME_ADDR_SHIFT: u32 = 20;
    pub const NAME_SIZE: usize = 12;

    pub const VFO_INFO_ADDR: u32 = 0x8000;
    pub const VFO_SIZE: u32 = 32;
    // pub const VFO1_ADDR: u32 = 0x8000;
    // pub const VFO2_ADDR: u32 = 0x8020;

    pub const RADIO_IMFOS_ADDR: u32 = 0x9000;
    // pub const RADIO_SIZE: u32 = 64;
    // pub const PWR_MSG_ADDR: u32 = 0x9040;

    pub const DTMFINFOR_ADDR: u32 = 0xA000;
    // pub const DTMF_KILLED_ADDR: u32 = 0xA010;
    pub const DTMF_CODE_ADDR: u32 = 0xA020;
    pub const CONTACT_SIZE: u32 = 16;
    pub const CONTACT_COUNT: usize = 20;
    // pub const DTMF_ONLINE_ADDR: u32 = 0xA180;
    // pub const DTMF_OFFLINE_ADDR: u32 = 0xA190;

    // pub const SCAN_LIST_ADDR: u32 = 0xB000;
    /// Reuses the factory's unused-by-us "scan list" sector to hold the
    /// first-boot marker
    pub const FIRST_BOOT_MARKER_ADDR: u32 = 0xB000;

    // pub const FM_IMFOS_ADDR: u32 = 0xC000;
    pub const FM_ADDR: u32 = 0xC000;

    // pub const RF_MODEL_ADDR: u32 = 0xD000;
    // pub const RF_TXEN_ADDR: u32 = 0xD001;

    pub const SYSTEMRAN_ADDR: u32 = 0xE000;
    // pub const F1CHAN_ADDR: u32 = 0xE000;
    // pub const F2CHAN_ADDR: u32 = 0xE002;

    // pub const DEBUG_ADDR: u32 = 0xF000;
    pub const PA_TABLE_BASE_HIGH: u32 = 0xF000;
    pub const PA_TABLE_BASE_MID: u32 = 0xF040;
    pub const PA_TABLE_BASE_LOW: u32 = 0xF080;
    pub const DEV_BATT_ADDR: u32 = 0xF200;
    // pub const RF_MODULATION_ADDR: u32 = 0xF210;
    // pub const DEV_LANGUAGE_ADDR: u32 = 0xF220;
    // pub const DEV_BTEN_ADDR: u32 = 0xF221;
    // pub const BAND_ADDR: u32 = 0xF230;
    // pub const BAND2_ADDR: u32 = 0xF240;
    // pub const MODEL_ADDR: u32 = 0xF250;

    pub const BOOT_LOGO_ADDR: u32 = 0x000C_0000;

    pub const BOOT_TEXT_ADDR: u32 = 0x9100;
    pub const BOOT_TUNE_ADDR: u32 = 0x9200;
    pub const BATTERY_CAL_ADDR: u32 = 0x9300;
    pub const APRS_SETTINGS_ADDR: u32 = 0x9400;

    pub const SAT_ADDR: u32 = 0x000D_0000; // voice prompt file for original fw

    pub const SSTV_IMAGE_ADDR: u32 = 0x000D_1000;
}

/// Maximum number of satellites stored in SPI flash.
pub const MAX_SATELLITES: usize = 20;

/// Written to `addr::FIRST_BOOT_MARKER_ADDR` once the first-boot format
/// prompt has run
pub const FIRST_BOOT_MAGIC: [u8; 4] = *b"AURA";

/// Raw byte size of the boot-logo bitmap at `addr::BOOT_LOGO_ADDR`: 8 pages
/// x 128 columns x 1 byte/column, the ST7565 controller's native page
/// format (bit0 = top row of the page).
pub const BOOT_LOGO_SIZE: usize = 1024;

/// Robot 36 SSTV image record footprint at `addr::SSTV_IMAGE_ADDR`: 240 rows
/// x (320 Y + 160 chroma) bytes/row, i.e. `app::sstv::tx::ROW_STRIDE *
/// app::sstv::tx::TOTAL_LINES`. Duplicated here as a plain literal (like
/// `BOOT_LOGO_SIZE`) so `device::storage` doesn't need to import from
/// `app`; `app::sstv::tx` has a compile-time assert keeping the two in
/// sync.
pub const SSTV_IMAGE_SIZE: usize = 115_200;

/// Both on-flash frequency encodings share the same logical unit: whole
/// decimal digits of deci-Hz (frequency in Hz, divided by 10). Channel
/// mode packs 2 digits/byte (BCD); VFO mode stores 1 digit/byte.
fn digits_to_deci_hz(digits: &[u8]) -> u32 {
    digits.iter().fold(0u32, |acc, &d| acc * 10 + d as u32)
}

fn deci_hz_to_digits(mut deci_hz: u32, out: &mut [u8]) {
    for slot in out.iter_mut().rev() {
        *slot = (deci_hz % 10) as u8;
        deci_hz /= 10;
    }
}

fn bcd_byte_to_decimal(b: u8) -> u32 {
    ((b >> 4) * 10 + (b & 0x0f)) as u32
}

fn decimal_to_bcd_byte(v: u32) -> u8 {
    (((v / 10) << 4) | (v % 10)) as u8
}

/// Memory order is little-endian (`bcd[0]` = least-significant 2 digits)
fn bcd4_to_deci_hz(bcd: [u8; 4]) -> u32 {
    bcd_byte_to_decimal(bcd[3]) * 1_000_000
        + bcd_byte_to_decimal(bcd[2]) * 10_000
        + bcd_byte_to_decimal(bcd[1]) * 100
        + bcd_byte_to_decimal(bcd[0])
}

fn deci_hz_to_bcd4(deci_hz: u32) -> [u8; 4] {
    [
        decimal_to_bcd_byte(deci_hz % 100),
        decimal_to_bcd_byte((deci_hz / 100) % 100),
        decimal_to_bcd_byte((deci_hz / 10_000) % 100),
        decimal_to_bcd_byte(deci_hz / 1_000_000),
    ]
}

/// FM broadcast channel list: stored plainly as tenths-of-a-MHz (650..=1080
/// fits easily in `u16`)
pub const FM_CHANNEL_COUNT: usize = 30;
/// Erased-flash sentinel for "slot not set"
pub const FM_CHANNEL_EMPTY: u16 = 0xFFFF;

/// Decoded meaning of the raw `rxDCSCTSNum`/`txDCSCTSNum` code, per
/// `GetCTSDCSType()`: 0 = none, 1..=105 = DCS normal, 106..=250 = DCS
/// inverted, >250 = CTCSS in tenths of Hz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubaudioCode {
    None,
    DcsNormal(u16),
    DcsInverted(u16),
    Ctcss(u16),
}

impl SubaudioCode {
    pub fn decode(raw: u16) -> SubaudioCode {
        if raw > 250 {
            SubaudioCode::Ctcss(raw)
        } else if raw >= 1 {
            if raw <= 105 {
                SubaudioCode::DcsNormal(raw)
            } else {
                SubaudioCode::DcsInverted(raw)
            }
        } else {
            SubaudioCode::None
        }
    }
}

/// On-flash channel record: `STR_CHANNEL` (20 bytes) followed by the
/// 12-byte name at `NAME_ADDR_SHIFT`, 32 bytes total (`CHAN_SIZE`).
#[derive(Clone, Copy)]
pub struct Channel {
    rx_freq_bcd: [u8; 4],
    tx_freq_bcd: [u8; 4],
    pub rx_dcs_cts_num: u16,
    pub tx_dcs_cts_num: u16,
    pub dtmf_group: u8,
    pub ptt_id: u8,
    pub tx_power: u8,
    flags: u8,
    pub decoder_code: u32,
    pub name: [u8; addr::NAME_SIZE],
}

impl Channel {
    pub fn from_bytes(buf: &[u8; addr::CHAN_SIZE as usize]) -> Channel {
        Channel {
            rx_freq_bcd: [buf[0], buf[1], buf[2], buf[3]],
            tx_freq_bcd: [buf[4], buf[5], buf[6], buf[7]],
            rx_dcs_cts_num: u16::from_le_bytes([buf[8], buf[9]]),
            tx_dcs_cts_num: u16::from_le_bytes([buf[10], buf[11]]),
            dtmf_group: buf[12],
            ptt_id: buf[13],
            tx_power: buf[14],
            flags: buf[15],
            decoder_code: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            name: buf[20..32].try_into().unwrap(),
        }
    }

    pub fn to_bytes(self) -> [u8; addr::CHAN_SIZE as usize] {
        let mut buf = [0u8; addr::CHAN_SIZE as usize];
        buf[0..4].copy_from_slice(&self.rx_freq_bcd);
        buf[4..8].copy_from_slice(&self.tx_freq_bcd);
        buf[8..10].copy_from_slice(&self.rx_dcs_cts_num.to_le_bytes());
        buf[10..12].copy_from_slice(&self.tx_dcs_cts_num.to_le_bytes());
        buf[12] = self.dtmf_group;
        buf[13] = self.ptt_id;
        buf[14] = self.tx_power;
        buf[15] = self.flags;
        buf[16..20].copy_from_slice(&self.decoder_code.to_le_bytes());
        buf[20..32].copy_from_slice(&self.name);
        buf
    }

    pub fn rx_freq_deci_hz(&self) -> u32 {
        bcd4_to_deci_hz(self.rx_freq_bcd)
    }

    pub fn set_rx_freq_deci_hz(&mut self, deci_hz: u32) {
        self.rx_freq_bcd = deci_hz_to_bcd4(deci_hz);
    }

    pub fn tx_freq_deci_hz(&self) -> u32 {
        bcd4_to_deci_hz(self.tx_freq_bcd)
    }

    pub fn set_tx_freq_deci_hz(&mut self, deci_hz: u32) {
        self.tx_freq_bcd = deci_hz_to_bcd4(deci_hz);
    }

    /// `chFlag3.Bit.b6`: 0 = wide (25K), 1 = narrow (12.5K).
    pub fn wide_narrow(&self) -> bool {
        self.flags & 0x40 != 0
    }

    pub fn set_wide_narrow(&mut self, narrow: bool) {
        if narrow {
            self.flags |= 0x40;
        } else {
            self.flags &= !0x40;
        }
    }

    /// `chFlag3.Bit.spMute`, bits 4-5: 0=QT 1=DTMF 2=QT+DTMF 3=QT*DTMF.
    #[allow(dead_code)]
    pub fn sp_mute(&self) -> u8 {
        (self.flags >> 4) & 0x03
    }

    /// `chFlag3.Bit.b3`: busy-channel-lockout.
    pub fn busy_lock(&self) -> bool {
        self.flags & 0x08 != 0
    }

    pub fn set_busy_lock(&mut self, on: bool) {
        if on {
            self.flags |= 0x08;
        } else {
            self.flags &= !0x08;
        }
    }

    /// `chFlag3.Bit.b2`: included in scan list.
    pub fn scan_add(&self) -> bool {
        self.flags & 0x04 != 0
    }

    pub fn set_scan_add(&mut self, on: bool) {
        if on {
            self.flags |= 0x04;
        } else {
            self.flags &= !0x04;
        }
    }

    pub fn ani_target(&self) -> Option<u8> {
        let idx = self.dtmf_group & 0x1f;
        if (idx as usize) < addr::CONTACT_COUNT {
            Some(idx)
        } else {
            None
        }
    }

    pub fn set_ani_target(&mut self, target: Option<u8>) {
        let idx = target.map_or(0x1f, |t| t.min(0x1f));
        self.dtmf_group = idx;
    }
}

#[derive(Clone, Copy)]
pub struct Contact {
    id_raw: [u8; 5],
    pub name: [u8; 11],
}

impl Contact {
    pub const BLANK: Contact = Contact {
        id_raw: [0xFF; 5],
        name: [0xFF; 11],
    };

    pub fn from_bytes(buf: &[u8; addr::CONTACT_SIZE as usize]) -> Contact {
        Contact {
            id_raw: buf[0..5].try_into().unwrap(),
            name: buf[5..16].try_into().unwrap(),
        }
    }

    pub fn to_bytes(self) -> [u8; addr::CONTACT_SIZE as usize] {
        let mut buf = [0u8; addr::CONTACT_SIZE as usize];
        buf[0..5].copy_from_slice(&self.id_raw);
        buf[5..16].copy_from_slice(&self.name);
        buf
    }

    pub fn id(&self) -> [u8; 3] {
        [self.id_raw[0], self.id_raw[1], self.id_raw[2]]
    }

    pub fn set_id(&mut self, id: [u8; 3]) {
        self.id_raw[0] = id[0];
        self.id_raw[1] = id[1];
        self.id_raw[2] = id[2];
    }

    pub fn is_empty(&self) -> bool {
        self.id_raw.iter().all(|&b| b == 0xFF)
    }
}

/// On-flash VFO record: `STR_VFOMODE`, 32 bytes (`VFO_SIZE`).
#[derive(Clone, Copy)]
pub struct VfoMode {
    freq_digits: [u8; 8],
    pub rx_dcs_cts_num: u16,
    pub tx_dcs_cts_num: u16,
    pub dtmf_group: u8,
    pub ani: u8,
    pub tx_power: u8,
    vfo_flags: u8,
    pub step: u8,
    offset_digits: [u8; 7],
    pub sp_mute: u8,
    pub decoder_code: u32,
}

impl VfoMode {
    pub fn from_bytes(buf: &[u8; addr::VFO_SIZE as usize]) -> VfoMode {
        VfoMode {
            freq_digits: buf[0..8].try_into().unwrap(),
            rx_dcs_cts_num: u16::from_le_bytes([buf[8], buf[9]]),
            tx_dcs_cts_num: u16::from_le_bytes([buf[10], buf[11]]),
            // buf[12..14] is `remain0`, unused.
            dtmf_group: buf[14],
            ani: buf[15],
            tx_power: buf[16],
            vfo_flags: buf[17],
            // buf[18] is `remain1`, unused.
            step: buf[19],
            offset_digits: buf[20..27].try_into().unwrap(),
            sp_mute: buf[27],
            decoder_code: u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]),
        }
    }

    pub fn to_bytes(self) -> [u8; addr::VFO_SIZE as usize] {
        let mut buf = [0u8; addr::VFO_SIZE as usize];
        buf[0..8].copy_from_slice(&self.freq_digits);
        buf[8..10].copy_from_slice(&self.rx_dcs_cts_num.to_le_bytes());
        buf[10..12].copy_from_slice(&self.tx_dcs_cts_num.to_le_bytes());
        buf[14] = self.dtmf_group;
        buf[15] = self.ani;
        buf[16] = self.tx_power;
        buf[17] = self.vfo_flags;
        buf[19] = self.step;
        buf[20..27].copy_from_slice(&self.offset_digits);
        buf[27] = self.sp_mute;
        buf[28..32].copy_from_slice(&self.decoder_code.to_le_bytes());
        buf
    }

    pub fn freq_deci_hz(&self) -> u32 {
        digits_to_deci_hz(&self.freq_digits)
    }

    pub fn set_freq_deci_hz(&mut self, deci_hz: u32) {
        deci_hz_to_digits(deci_hz, &mut self.freq_digits);
    }

    /// Repeater shift, in deci-Hz; the on-flash digits carry one less
    /// digit of precision than the frequency itself (`VfoOffsetCalculate`
    /// multiplies the parsed digits by 10).
    pub fn offset_deci_hz(&self) -> u32 {
        digits_to_deci_hz(&self.offset_digits) * 10
    }

    pub fn set_offset_deci_hz(&mut self, deci_hz: u32) {
        deci_hz_to_digits(deci_hz / 10, &mut self.offset_digits);
    }

    /// `dtmfgroup`'s top 3 bits double up as the repeater shift direction
    /// (0 = simplex, 1 = +, 2 = -)
    pub fn freq_dir(&self) -> u8 {
        self.dtmf_group >> 5
    }

    pub fn set_freq_dir(&mut self, dir: u8) {
        self.dtmf_group = (dir << 5) | (self.dtmf_group & 0x1f);
    }

    /// `dtmfgroup`'s low 5 bits, same convention as `Channel::ani_target`.
    pub fn ani_target(&self) -> Option<u8> {
        let idx = self.dtmf_group & 0x1f;
        if (idx as usize) < addr::CONTACT_COUNT {
            Some(idx)
        } else {
            None
        }
    }

    pub fn set_ani_target(&mut self, target: Option<u8>) {
        let idx = target.map_or(0x1f, |t| t.min(0x1f));
        self.dtmf_group = (self.dtmf_group & 0xe0) | idx;
    }

    /// `vfoFlag.Bit.b6`: 0 = wide (25K), 1 = narrow (12.5K).
    pub fn wide_narrow(&self) -> bool {
        self.vfo_flags & 0x40 != 0
    }

    pub fn set_wide_narrow(&mut self, narrow: bool) {
        if narrow {
            self.vfo_flags |= 0x40;
        } else {
            self.vfo_flags &= !0x40;
        }
    }
}

/// On-flash satellite record: `MAX_SATELLITES` slots of `SAT_RECORD_SIZE`
/// bytes each, starting at `addr::SAT_ADDR`.
#[derive(Clone, Copy)]
pub struct SatRecord {
    pub name: [u8; 10],
    /// Downlink (RX) frequency in Hz.
    pub rx_freq_hz: u32,
    /// Uplink (TX) frequency in Hz. 0 means RX-only.
    pub tx_freq_hz: u32,
    /// Downlink subaudio tone in tenths of Hz (0 = none).
    pub rx_tone_hz: u16,
    /// Uplink subaudio tone in tenths of Hz (0 = none).
    pub tx_tone_hz: u16,
    /// Orbital altitude in km.
    pub altitude_km: u16,
    /// Reserved flags.
    pub flags: u8,
    _pad: [u8; 3],
}

pub const SAT_RECORD_SIZE: usize = 32;

impl SatRecord {
    pub const BLANK: SatRecord = SatRecord {
        name: [0; 10],
        rx_freq_hz: 0,
        tx_freq_hz: 0,
        rx_tone_hz: 0,
        tx_tone_hz: 0,
        altitude_km: 0,
        flags: 0,
        _pad: [0; 3],
    };

    pub fn from_bytes(buf: &[u8; SAT_RECORD_SIZE]) -> Option<SatRecord> {
        if buf.iter().all(|&b| b == 0xFF) {
            return None;
        }
        let mut name = [0u8; 10];
        name.copy_from_slice(&buf[0..10]);
        Some(SatRecord {
            name,
            rx_freq_hz: u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]),
            tx_freq_hz: u32::from_le_bytes([buf[14], buf[15], buf[16], buf[17]]),
            rx_tone_hz: u16::from_le_bytes([buf[18], buf[19]]),
            tx_tone_hz: u16::from_le_bytes([buf[20], buf[21]]),
            altitude_km: u16::from_le_bytes([buf[22], buf[23]]),
            flags: buf[24],
            _pad: [buf[25], buf[26], buf[27]],
        })
    }

    pub fn to_bytes(self) -> [u8; SAT_RECORD_SIZE] {
        let mut buf = [0u8; SAT_RECORD_SIZE];
        buf[0..10].copy_from_slice(&self.name);
        buf[10..14].copy_from_slice(&self.rx_freq_hz.to_le_bytes());
        buf[14..18].copy_from_slice(&self.tx_freq_hz.to_le_bytes());
        buf[18..20].copy_from_slice(&self.rx_tone_hz.to_le_bytes());
        buf[20..22].copy_from_slice(&self.tx_tone_hz.to_le_bytes());
        buf[22..24].copy_from_slice(&self.altitude_km.to_le_bytes());
        buf[24] = self.flags;
        // last 7 bytes are padding (already 0)
        buf
    }
}

#[derive(Clone, Copy)]
pub struct Settings {
    /// `sqlLevel`, 0-9.
    pub sql_level: u8,
    /// `tailSwitch`: squelch-tail elimination on TX release.
    pub tail_elimination: bool,
    /// `txBusyLock`: block PTT while the channel is busy.
    pub busy_lock: bool,
    /// `txForbid`: global TX inhibit.
    pub tx_forbid: bool,
    /// `keyAutoLock`: idle time before the keypad locks itself. The menu
    /// presents this as OFF/5S/10S/15S; the stored value is the index,
    /// 0 (off) - 3. Lock delay is `value * 5` seconds.
    pub key_auto_lock: u8,
    /// `dualRxFlag` != 0: dual-standby enabled.
    pub dual_standby: bool,
    /// `voxSwitch`.
    pub vox_switch: bool,
    /// `voxLevel`, 1-9 (meaningless while `vox_switch` is off).
    pub vox_level: u8,
    /// `totLevel`, steps of 15s; 0 = disabled.
    pub tot_level: u8,
    /// `beepsSwitch`: audible tone on keypress.
    pub beeps_switch: bool,
    /// `txOffTone`: what plays right after PTT release, before the tail
    /// elimination tone (if any). 0 = off, 1 = roger beep, 2 = MDC1200 burst.
    pub roger_tone: u8,
    /// `scarmble`: voice-inversion scramble group, 0 (off) - 3.
    pub scramble_level: u8,
    /// LCD electronic-volume level, 0-4
    pub contrast: u8,
    /// Repeater-access tone selection, 0-3: which single tone a side-key
    /// press sends during TX. 1000/1450/1750/2100 Hz; default is 1750 Hz.
    pub rtone: u8,
    /// Channel/frequency scan resume behavior, 0-2: 0 = Time (fixed dwell,
    /// then resume regardless of carrier), 1 = Carrier (resume a fixed
    /// grace period after carrier drops), 2 = Search (stop scanning
    /// entirely once found).
    pub scan_mode: u8,
    /// Receiver incremental tuning (clarifier), only applied while
    /// `Modulation::Usb` is active: units of 10Hz (the hardware's own
    /// minimum tuning step), full `i8` range so +/-127 steps = +/-1270Hz.
    pub rit_offset: i8,
    /// RX duty-cycle power-save level, 0 (off) - 4. Controls how long the
    /// RFIC is powered down between brief wake-and-listen samples
    pub save_level: u8,
    /// Automatic backlight-off delay ("ABR"), 0-4: 0 = always on (no
    /// timeout), 1..4 = 5/10/15/20 seconds of idle before the backlight
    /// turns off. Forced back on regardless of this while transmitting.
    pub backlight_time: u8,
    /// Standby-screen channel readout, 0-2: 0 = frequency only, 1 = channel
    /// name only, 2 = name and freq.
    pub channel_display_mode: u8,
    /// Programmable-key function assignments: each stores a
    /// `app::keyfn::KeyFunction` index (0..=9). `side2_short`/`band_short`/
    /// `band_long` default to non-`None` to preserve this firmware's
    /// pre-existing hardcoded key bindings (Side2 TX-tone, Band MODE/Search).
    pub side1_short: u8,
    pub side1_long: u8,
    pub side2_short: u8,
    pub side2_long: u8,
    pub band_short: u8,
    pub band_long: u8,
    /// Auto-send our own ANI frame (`[call target] + separator +
    /// [machine id]`) on PTT press. The call target itself is per-side
    /// (`Channel`/`VfoMode::ani_target`, or a runtime override); this is
    /// just the master on/off switch.
    pub ani_tx: bool,
    /// `rptrl`: 0-10, steps of 100ms (0-1000ms). Holds the receiver muted
    /// for this long after our own PTT release, riding out a repeater's
    /// hang time/courtesy tone before re-arming the squelch.
    pub rptrl: u8,
    /// `voxDelay`: 0-15, steps of 0.1s over 0.5-2.0s. How long VOX keeps
    /// PTT keyed after mic level drops back below threshold.
    pub vox_delay: u8,
    /// Boot-screen content, 0-3: 0 = none (skip straight to standby; also
    /// forces `boot_sound_enabled` off, see `app::settings_ops::apply`),
    /// 1 = battery voltage (`app::App::battery_voltage_cv`), 2 = custom text
    /// (`boot_text_line1`/`2`), 3 = stored logo bitmap
    /// (`addr::BOOT_LOGO_ADDR`).
    pub boot_display_mode: u8,
    /// Play `boot_tune` through the tone oscillator during boot. Forced to
    /// `false` whenever `boot_display_mode` is set to 0 (None).
    pub boot_sound_enabled: bool,
    /// Two 16-character ASCII lines shown in boot display modes 1/3,
    /// CPS-editable. A 0x00 or 0xFF byte terminates a line early.
    pub boot_text_line1: [u8; 16],
    pub boot_text_line2: [u8; 16],
    /// OpenGD77-compatible boot melody: 48 `(tone_index, duration)` pairs.
    /// `tone_index` 0 = rest, 1..=45 indexes the standard chromatic scale
    /// (`device::radio::BOOT_TUNE_HZ_DIV_10`); `duration` is in units of
    /// 30ms. A `(0, 0)` pair before the 48th slot ends the tune early.
    pub boot_tune: [(u8, u8); 48],
    /// Raw 12-bit ADC calibration point for `app::App::battery_voltage_cv`:
    /// the battery-ADC raw reading
    pub battery_cal_raw: u16,
    /// Country/region TX frequency lock: a `device::radio::BandLock` index
    /// (0..=9). Applied to `Radio::set_tx_allowed` on boot and whenever
    /// changed in Settings; RX is never restricted by this.
    pub band_lock: u8,
    /// Break-in behavior while the master side's modulation is CW/CWF
    /// 0 = off, 1 = semi, 2 = full
    pub bk_in: u8,
    /// APRS callsign (6 chars + null terminator), e.g. "BG4KNN\0".
    pub aprs_callsign: [u8; 7],
    /// APRS latitude, degrees * 100_000 (negative = south).
    pub aprs_lat: i32,
    /// APRS longitude, degrees * 100_000 (negative = west).
    pub aprs_lon: i32,
    /// Index into `app::aprs_ops::DIGIPATH_PRESETS`.
    pub aprs_path_idx: u8,
    /// APRS TX frequency in Hz
    pub aprs_freq_hz: u32,
    /// Include device model name in APRS beacon comment.
    pub aprs_dev_info: bool,
    /// Device model name (6 chars + null), e.g. "UV-K6x\0".
    pub aprs_dev_name: [u8; 7],
    /// Include battery voltage in APRS beacon comment.
    pub aprs_bat_volt: bool,
    /// Custom APRS comment text (16 chars + null).
    pub aprs_custom_comment: [u8; 17],
    /// APRS source SSID (0–15), e.g. 7 for "BG4KNN-7".
    pub aprs_ssid: u8,
    /// Index into `APRS_SYMBOLS` preset table.
    pub aprs_symbol_idx: u8,
    /// APRS TX power: 0 = Low, 1 = Mid, 2 = High.
    pub aprs_power: u8,
}

/// Size of the serialised Settings record in bytes.
pub const SETTINGS_BYTES: usize = 212;

/// Sentinel meaning "APRS coordinates not yet configured."
/// Deliberately outside valid [-90°, 90°] / [-180°, 180°] range.
pub const APRS_COORD_NOT_SET: i32 = i32::MAX;

impl Settings {
    pub const DEFAULT: Settings = Settings {
        sql_level: 3,
        tail_elimination: true,
        busy_lock: false,
        tx_forbid: false,
        key_auto_lock: 0,
        dual_standby: true,
        vox_switch: false,
        vox_level: 1,
        tot_level: 8,
        beeps_switch: true,
        roger_tone: 0,
        scramble_level: 0,
        contrast: 2,
        rtone: 2,
        scan_mode: 1,
        rit_offset: 0,
        save_level: 0,
        backlight_time: 2,
        channel_display_mode: 2,
        side1_short: 8, // Flashlight
        side1_long: 7,  // Power
        side2_short: 2, // Monitor
        side2_long: 3,  // Mode
        band_short: 9,  // Search
        band_long: 0,   // None
        ani_tx: false,
        rptrl: 0,
        vox_delay: 5, // 1.0s, matching the previous hardcoded hang time
        boot_display_mode: 2,
        boot_sound_enabled: false,
        boot_text_line1: [0; 16],
        boot_text_line2: [0; 16],
        boot_tune: [(0, 0); 48],
        battery_cal_raw: 2731,
        band_lock: 0, // CE & CN
        bk_in: 0,
        aprs_callsign: [0; 7],
        aprs_lat: APRS_COORD_NOT_SET,
        aprs_lon: APRS_COORD_NOT_SET,
        aprs_path_idx: 0, // WIDE1-1,WIDE2-1
        aprs_freq_hz: 144_390_000,
        aprs_dev_info: true,
        aprs_dev_name: *b"UV-K6x\x00",
        aprs_bat_volt: false,
        aprs_custom_comment: [0; 17],
        aprs_ssid: 0,
        aprs_symbol_idx: 0,
        aprs_power: 0, // Low
    };

    pub fn from_bytes(buf: &[u8; SETTINGS_BYTES]) -> Settings {
        let mut boot_text_line1 = [0u8; 16];
        boot_text_line1.copy_from_slice(&buf[30..46]);
        let mut boot_text_line2 = [0u8; 16];
        boot_text_line2.copy_from_slice(&buf[46..62]);
        let mut boot_tune = [(0u8, 0u8); 48];
        for (i, pair) in boot_tune.iter_mut().enumerate() {
            *pair = (buf[62 + i * 2], buf[62 + i * 2 + 1]);
        }
        let mut aprs_call = [0u8; 7];
        aprs_call.copy_from_slice(&buf[163..170]);
        let mut aprs_dev_name = [0u8; 7];
        aprs_dev_name.copy_from_slice(&buf[184..191]);
        let mut aprs_custom_comment = [0u8; 17];
        aprs_custom_comment.copy_from_slice(&buf[192..209]);

        Settings {
            sql_level: buf[0],
            tail_elimination: buf[1] != 0,
            busy_lock: buf[2] != 0,
            tx_forbid: buf[3] != 0,
            key_auto_lock: buf[4].min(3),
            dual_standby: buf[5] != 0,
            vox_switch: buf[6] != 0,
            vox_level: buf[7].clamp(1, 9),
            tot_level: buf[8],
            beeps_switch: buf[9] != 0,
            roger_tone: buf[10].min(2),
            scramble_level: buf[11],
            contrast: buf[12].min(4),
            rtone: buf[13].min(3),
            scan_mode: buf[14].min(2),
            rit_offset: (buf[15] as i8).clamp(-127, 127),
            save_level: buf[16].min(4),
            backlight_time: buf[17].min(4),
            channel_display_mode: buf[18].min(2),
            side1_short: buf[19].min(11),
            side1_long: buf[20].min(11),
            side2_short: buf[21].min(11),
            side2_long: buf[22].min(11),
            band_short: buf[23].min(11),
            band_long: buf[24].min(11),
            ani_tx: buf[25] != 0,
            rptrl: buf[26].min(10),
            vox_delay: buf[27].min(15),
            boot_display_mode: buf[28].min(3),
            boot_sound_enabled: buf[29] != 0,
            boot_text_line1,
            boot_text_line2,
            boot_tune,
            battery_cal_raw: u16::from_le_bytes([buf[158], buf[159]]),
            band_lock: buf[160].min(9),
            bk_in: buf[162].min(2),
            aprs_callsign: aprs_call,
            aprs_lat: {
                let v = i32::from_le_bytes([buf[170], buf[171], buf[172], buf[173]]);
                if (-90_00000..=90_00000).contains(&v) {
                    v
                } else {
                    APRS_COORD_NOT_SET
                }
            },
            aprs_lon: {
                let v = i32::from_le_bytes([buf[174], buf[175], buf[176], buf[177]]);
                if (-180_00000..=180_00000).contains(&v) {
                    v
                } else {
                    APRS_COORD_NOT_SET
                }
            },
            aprs_path_idx: buf[178].min(3),
            aprs_freq_hz: u32::from_le_bytes([buf[179], buf[180], buf[181], buf[182]]),
            aprs_dev_info: buf[183] != 0,
            aprs_dev_name,
            aprs_bat_volt: buf[191] != 0,
            aprs_custom_comment,
            aprs_ssid: buf[209].min(15),
            aprs_symbol_idx: buf[210].min(10),
            aprs_power: buf[211].min(2),
        }
    }

    pub fn to_bytes(self) -> [u8; SETTINGS_BYTES] {
        let mut buf = [0u8; SETTINGS_BYTES];
        buf[0] = self.sql_level;
        buf[1] = self.tail_elimination as u8;
        buf[2] = self.busy_lock as u8;
        buf[3] = self.tx_forbid as u8;
        buf[4] = self.key_auto_lock;
        buf[5] = self.dual_standby as u8;
        buf[6] = self.vox_switch as u8;
        buf[7] = self.vox_level;
        buf[8] = self.tot_level;
        buf[9] = self.beeps_switch as u8;
        buf[10] = self.roger_tone;
        buf[11] = self.scramble_level;
        buf[12] = self.contrast;
        buf[13] = self.rtone;
        buf[14] = self.scan_mode;
        buf[15] = self.rit_offset as u8;
        buf[16] = self.save_level;
        buf[17] = self.backlight_time;
        buf[18] = self.channel_display_mode;
        buf[19] = self.side1_short;
        buf[20] = self.side1_long;
        buf[21] = self.side2_short;
        buf[22] = self.side2_long;
        buf[23] = self.band_short;
        buf[24] = self.band_long;
        buf[25] = self.ani_tx as u8;
        buf[26] = self.rptrl;
        buf[27] = self.vox_delay;
        buf[28] = self.boot_display_mode;
        buf[29] = self.boot_sound_enabled as u8;
        buf[30..46].copy_from_slice(&self.boot_text_line1);
        buf[46..62].copy_from_slice(&self.boot_text_line2);
        for (i, &(tone, duration)) in self.boot_tune.iter().enumerate() {
            buf[62 + i * 2] = tone;
            buf[62 + i * 2 + 1] = duration;
        }
        buf[158..160].copy_from_slice(&self.battery_cal_raw.to_le_bytes());
        buf[160] = self.band_lock;
        buf[162] = self.bk_in;
        // APRS fields start at offset 163
        buf[163..170].copy_from_slice(&self.aprs_callsign);
        buf[170..174].copy_from_slice(&self.aprs_lat.to_le_bytes());
        buf[174..178].copy_from_slice(&self.aprs_lon.to_le_bytes());
        buf[178] = self.aprs_path_idx;
        buf[179..183].copy_from_slice(&self.aprs_freq_hz.to_le_bytes());
        // APRS comment fields (new in v2)
        buf[183] = self.aprs_dev_info as u8;
        buf[184..191].copy_from_slice(&self.aprs_dev_name);
        buf[191] = self.aprs_bat_volt as u8;
        buf[192..209].copy_from_slice(&self.aprs_custom_comment);
        buf[209] = self.aprs_ssid;
        buf[210] = self.aprs_symbol_idx;
        buf[211] = self.aprs_power;
        buf
    }

    /// TOT in seconds; 0 = disabled.
    pub fn tot_seconds(&self) -> u16 {
        self.tot_level as u16 * 15
    }
}
