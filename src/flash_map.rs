pub mod addr {
    pub const CHAN_ADDR: u32 = 0x0000;
    pub const CHAN_SIZE: u32 = 32;
    pub const NAME_ADDR_SHIFT: u32 = 20;
    pub const NAME_SIZE: usize = 12;

    pub const VFO_INFO_ADDR: u32 = 0x8000;
    pub const VFO_SIZE: u32 = 32;
    pub const VFO1_ADDR: u32 = 0x8000;
    pub const VFO2_ADDR: u32 = 0x8020;

    pub const RADIO_IMFOS_ADDR: u32 = 0x9000;
    pub const RADIO_SIZE: u32 = 64;
    pub const PWR_MSG_ADDR: u32 = 0x9040;

    pub const DTMFINFOR_ADDR: u32 = 0xA000;
    pub const DTMF_KILLED_ADDR: u32 = 0xA010;
    pub const DTMF_CODE_ADDR: u32 = 0xA020;
    pub const CONTACT_SIZE: u32 = 16;
    pub const CONTACT_COUNT: usize = 20;
    pub const DTMF_ONLINE_ADDR: u32 = 0xA180;
    pub const DTMF_OFFLINE_ADDR: u32 = 0xA190;

    pub const SCAN_LIST_ADDR: u32 = 0xB000;

    pub const FM_IMFOS_ADDR: u32 = 0xC000;
    pub const FM_ADDR: u32 = 0xC000;

    pub const RF_MODEL_ADDR: u32 = 0xD000;
    pub const RF_TXEN_ADDR: u32 = 0xD001;

    pub const SYSTEMRAN_ADDR: u32 = 0xE000;
    pub const F1CHAN_ADDR: u32 = 0xE000;
    pub const F2CHAN_ADDR: u32 = 0xE002;

    pub const DEBUG_ADDR: u32 = 0xF000;
    pub const PA_TABLE_BASE_HIGH: u32 = 0xF000;
    pub const PA_TABLE_BASE_MID: u32 = 0xF040;
    pub const PA_TABLE_BASE_LOW: u32 = 0xF080;
    pub const DEV_BATT_ADDR: u32 = 0xF200;
    pub const RF_MODULATION_ADDR: u32 = 0xF210;
    pub const DEV_LANGUAGE_ADDR: u32 = 0xF220;
    pub const DEV_BTEN_ADDR: u32 = 0xF221;
    pub const BAND_ADDR: u32 = 0xF230;
    pub const BAND2_ADDR: u32 = 0xF240;
    pub const MODEL_ADDR: u32 = 0xF250;
}

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

    pub fn raw(self) -> u16 {
        match self {
            SubaudioCode::None => 0,
            SubaudioCode::DcsNormal(c) | SubaudioCode::DcsInverted(c) | SubaudioCode::Ctcss(c) => c,
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

    pub fn to_bytes(&self) -> [u8; addr::CHAN_SIZE as usize] {
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

    pub fn to_bytes(&self) -> [u8; addr::CONTACT_SIZE as usize] {
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

    pub fn to_bytes(&self) -> [u8; addr::VFO_SIZE as usize] {
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
}

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
    };

    pub fn from_bytes(buf: &[u8; 28]) -> Settings {
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
            side1_short: buf[19].min(10),
            side1_long: buf[20].min(10),
            side2_short: buf[21].min(10),
            side2_long: buf[22].min(10),
            band_short: buf[23].min(10),
            band_long: buf[24].min(10),
            ani_tx: buf[25] != 0,
            rptrl: buf[26].min(10),
            vox_delay: buf[27].min(15),
        }
    }

    pub fn to_bytes(&self) -> [u8; 28] {
        [
            self.sql_level,
            self.tail_elimination as u8,
            self.busy_lock as u8,
            self.tx_forbid as u8,
            self.key_auto_lock,
            self.dual_standby as u8,
            self.vox_switch as u8,
            self.vox_level,
            self.tot_level,
            self.beeps_switch as u8,
            self.roger_tone,
            self.scramble_level,
            self.contrast,
            self.rtone,
            self.scan_mode,
            self.rit_offset as u8,
            self.save_level,
            self.backlight_time,
            self.channel_display_mode,
            self.side1_short,
            self.side1_long,
            self.side2_short,
            self.side2_long,
            self.band_short,
            self.band_long,
            self.ani_tx as u8,
            self.rptrl,
            self.vox_delay,
        ]
    }

    /// TOT in seconds; 0 = disabled.
    pub fn tot_seconds(&self) -> u16 {
        self.tot_level as u16 * 15
    }
}
