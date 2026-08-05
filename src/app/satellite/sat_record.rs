use crate::flash_map;

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

    pub fn to_bytes(&self) -> [u8; SAT_RECORD_SIZE] {
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

    /// True if this record has a non-empty name.
    pub fn is_defined(&self) -> bool {
        self.name.iter().any(|&b| b != 0 && b != 0xFF)
    }

    pub const PAYLOAD_LEN: usize = flash_map::MAX_SATELLITES * SAT_RECORD_SIZE;
}
