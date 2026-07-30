#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingItem {
    Sql,
    Step,
    Tot,
    Tdr,
    Save,
    Abr,
    BusyLock,
    TxForbid,
    Wn,
    TxPr,
    RxCts,
    TxCts,
    Sftd,
    Offse,
    Beep,
    AutoLk,
    Vox,
    VoxLv,
    Scrm,
    Rtone,
    Rptrl,
    Contrast,
    Roge,
    ScanMd,
    Rit,
    Info,
    Reset,
}

pub const SETTINGS_ORDER: [SettingItem; 27] = [
    SettingItem::Sql,
    SettingItem::Step,
    SettingItem::Tot,
    SettingItem::Tdr,
    SettingItem::Save,
    SettingItem::Abr,
    SettingItem::BusyLock,
    SettingItem::TxForbid,
    SettingItem::Wn,
    SettingItem::TxPr,
    SettingItem::RxCts,
    SettingItem::TxCts,
    SettingItem::Sftd,
    SettingItem::Offse,
    SettingItem::Beep,
    SettingItem::AutoLk,
    SettingItem::Vox,
    SettingItem::VoxLv,
    SettingItem::Scrm,
    SettingItem::Rtone,
    SettingItem::Rptrl,
    SettingItem::Contrast,
    SettingItem::Roge,
    SettingItem::ScanMd,
    SettingItem::Rit,
    SettingItem::Info,
    SettingItem::Reset,
];

impl SettingItem {
    pub fn label(self) -> &'static str {
        match self {
            SettingItem::Sql => "SQL",
            SettingItem::Step => "STEP",
            SettingItem::Tot => "TOT",
            SettingItem::Tdr => "TDR",
            SettingItem::Save => "SAVE",
            SettingItem::Abr => "ABR",
            SettingItem::BusyLock => "BCL",
            SettingItem::TxForbid => "TXINH",
            SettingItem::Wn => "W/N",
            SettingItem::TxPr => "PWR",
            SettingItem::RxCts => "R-CTC",
            SettingItem::TxCts => "T-CTC",
            SettingItem::Sftd => "SHIFT",
            SettingItem::Offse => "OFFSET",
            SettingItem::Beep => "BEEP",
            SettingItem::AutoLk => "AUTOLK",
            SettingItem::Vox => "VOX",
            SettingItem::VoxLv => "VOXLV",
            SettingItem::Scrm => "SCRM",
            SettingItem::Rtone => "RTONE",
            SettingItem::Rptrl => "RPTRL",
            SettingItem::Contrast => "CONTR",
            SettingItem::Roge => "ROGER",
            SettingItem::ScanMd => "SCANMD",
            SettingItem::Rit => "RIT",
            SettingItem::Info => "INFO",
            SettingItem::Reset => "RESET",
        }
    }

    pub fn is_placeholder(self) -> bool {
        matches!(self, SettingItem::Rptrl)
    }
}

/// Standard CTCSS tones, tenths of Hz (67.0Hz..254.1Hz), for RXCTS/TXCTS to
/// step through. Index `usize::MAX` (via `Option::None`) means "off".
pub const CTCSS_TABLE: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188,
    1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738, 1773, 1799,
    1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418, 2503, 2541,
];

/// `None` = off; `Some(i)` = index into `CTCSS_TABLE`.
pub fn ctcss_index(tenths_hz: Option<u16>) -> Option<usize> {
    tenths_hz.and_then(|hz| CTCSS_TABLE.iter().position(|&t| t == hz))
}

/// Standard DCS tone codes (octal, plain-binary value), for R-CTC/T-CTC to
/// step through after the CTCSS tones
pub const DCS_TABLE: [u16; 105] = [
    0o023, 0o025, 0o026, 0o031, 0o032, 0o036, 0o043, 0o047, 0o051, 0o053, 0o054, 0o065, 0o071,
    0o072, 0o073, 0o074, 0o114, 0o115, 0o116, 0o122, 0o125, 0o131, 0o132, 0o134, 0o143, 0o145,
    0o152, 0o155, 0o156, 0o162, 0o165, 0o172, 0o174, 0o205, 0o212, 0o223, 0o225, 0o226, 0o243,
    0o244, 0o245, 0o246, 0o251, 0o252, 0o255, 0o261, 0o263, 0o265, 0o266, 0o271, 0o274, 0o306,
    0o311, 0o315, 0o325, 0o331, 0o332, 0o343, 0o346, 0o351, 0o356, 0o364, 0o365, 0o371, 0o411,
    0o412, 0o413, 0o423, 0o431, 0o432, 0o445, 0o446, 0o452, 0o454, 0o455, 0o462, 0o464, 0o465,
    0o466, 0o503, 0o506, 0o516, 0o523, 0o526, 0o532, 0o546, 0o565, 0o606, 0o612, 0o624, 0o627,
    0o631, 0o632, 0o645, 0o654, 0o662, 0o664, 0o703, 0o712, 0o723, 0o731, 0o732, 0o734, 0o743,
    0o754,
];

/// `None` = index into `DCS_TABLE`, not found.
pub fn dcs_index(code: u16) -> Option<usize> {
    DCS_TABLE.iter().position(|&c| c == code)
}
