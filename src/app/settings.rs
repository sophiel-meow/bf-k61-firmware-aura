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
    VoxDly,
    Scrm,
    Rtone,
    Tail,
    Rptrl,
    Contrast,
    Roge,
    ScanMd,
    Rit,
    ChDisp,
    AniTx,
    AniCall,
    Side1Short,
    Side1Long,
    Side2Short,
    Side2Long,
    BandShort,
    BandLong,
    BkIn,
    BootMode,
    BootSnd,
    BattCal,
    FLock,
    Info,
    Reset,
    // APRS
    AprsCall,
    AprsFreq,
    AprsLat,
    AprsLon,
    AprsPath,
    AprsDevInfo,
    AprsDevName,
    AprsBatVolt,
    AprsComment,
    AprsSsid,
    AprsSymbol,
    AprsPower,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsGroup {
    Radio,
    Sig,
    Display,
    Keys,
    Ani,
    Aprs,
    System,
}

pub const SETTINGS_GROUPS: [SettingsGroup; 7] = [
    SettingsGroup::Radio,
    SettingsGroup::Sig,
    SettingsGroup::Display,
    SettingsGroup::Keys,
    SettingsGroup::Ani,
    SettingsGroup::Aprs,
    SettingsGroup::System,
];

impl SettingsGroup {
    pub fn label(self) -> &'static str {
        match self {
            SettingsGroup::Radio => "RADIO",
            SettingsGroup::Sig => "SIG",
            SettingsGroup::Display => "DISP",
            SettingsGroup::Keys => "KEYS",
            SettingsGroup::Ani => "ANI",
            SettingsGroup::Aprs => "DIGI",
            SettingsGroup::System => "SYSTEM",
        }
    }

    pub fn items(self) -> &'static [SettingItem] {
        match self {
            SettingsGroup::Radio => &[
                SettingItem::Sql,
                SettingItem::Step,
                SettingItem::Tot,
                SettingItem::Tdr,
                SettingItem::Save,
                SettingItem::BusyLock,
                SettingItem::TxForbid,
                SettingItem::Wn,
                SettingItem::TxPr,
                SettingItem::RxCts,
                SettingItem::TxCts,
                SettingItem::Sftd,
                SettingItem::Offse,
                SettingItem::Scrm,
                SettingItem::ScanMd,
                SettingItem::Rit,
                SettingItem::ChDisp,
                SettingItem::BkIn,
            ],
            SettingsGroup::Sig => &[
                SettingItem::Beep,
                SettingItem::Vox,
                SettingItem::VoxLv,
                SettingItem::VoxDly,
                SettingItem::Rtone,
                SettingItem::Tail,
                SettingItem::Rptrl,
                SettingItem::Roge,
            ],
            SettingsGroup::Display => {
                &[SettingItem::Abr, SettingItem::AutoLk, SettingItem::Contrast]
            }
            SettingsGroup::Keys => &[
                SettingItem::Side1Short,
                SettingItem::Side1Long,
                SettingItem::Side2Short,
                SettingItem::Side2Long,
                SettingItem::BandShort,
                SettingItem::BandLong,
            ],
            SettingsGroup::Ani => &[SettingItem::AniTx, SettingItem::AniCall],
            SettingsGroup::Aprs => &[
                SettingItem::AprsCall,
                SettingItem::AprsFreq,
                SettingItem::AprsLat,
                SettingItem::AprsLon,
                SettingItem::AprsPath,
                SettingItem::AprsDevInfo,
                SettingItem::AprsDevName,
                SettingItem::AprsBatVolt,
                SettingItem::AprsComment,
                SettingItem::AprsSsid,
                SettingItem::AprsSymbol,
                SettingItem::AprsPower,
            ],
            SettingsGroup::System => &[
                SettingItem::FLock,
                SettingItem::BattCal,
                SettingItem::BootMode,
                SettingItem::BootSnd,
                SettingItem::Info,
                SettingItem::Reset,
            ],
        }
    }
}

#[allow(dead_code)]
pub const SETTINGS_ORDER: [SettingItem; 55] = [
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
    SettingItem::VoxDly,
    SettingItem::Scrm,
    SettingItem::Rtone,
    SettingItem::Tail,
    SettingItem::Rptrl,
    SettingItem::Contrast,
    SettingItem::Roge,
    SettingItem::ScanMd,
    SettingItem::Rit,
    SettingItem::ChDisp,
    SettingItem::AniTx,
    SettingItem::AniCall,
    SettingItem::Side1Short,
    SettingItem::Side1Long,
    SettingItem::Side2Short,
    SettingItem::Side2Long,
    SettingItem::BandShort,
    SettingItem::BandLong,
    SettingItem::BkIn,
    SettingItem::BootMode,
    SettingItem::BootSnd,
    SettingItem::BattCal,
    SettingItem::FLock,
    SettingItem::Info,
    SettingItem::Reset,
    // APRS
    SettingItem::AprsCall,
    SettingItem::AprsFreq,
    SettingItem::AprsLat,
    SettingItem::AprsLon,
    SettingItem::AprsPath,
    SettingItem::AprsDevInfo,
    SettingItem::AprsDevName,
    SettingItem::AprsBatVolt,
    SettingItem::AprsComment,
    SettingItem::AprsSsid,
    SettingItem::AprsSymbol,
    SettingItem::AprsPower,
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
            SettingItem::VoxDly => "VOXDLY",
            SettingItem::Scrm => "SCRM",
            SettingItem::Rtone => "RTONE",
            SettingItem::Tail => "STE",
            SettingItem::Rptrl => "RPTRL",
            SettingItem::Contrast => "CONTR",
            SettingItem::Roge => "ROGER",
            SettingItem::ScanMd => "SCANMD",
            SettingItem::Rit => "RIT",
            SettingItem::ChDisp => "CHDISP",
            SettingItem::AniTx => "ANI-TX",
            SettingItem::AniCall => "CALL",
            SettingItem::Side1Short => "S1-SH",
            SettingItem::Side1Long => "S1-LG",
            SettingItem::Side2Short => "S2-SH",
            SettingItem::Side2Long => "S2-LG",
            SettingItem::BandShort => "BND-SH",
            SettingItem::BandLong => "BND-LG",
            SettingItem::BkIn => "BK-IN",
            SettingItem::BootMode => "BOOTSCR",
            SettingItem::BootSnd => "BOOTSND",
            SettingItem::BattCal => "BATCAL",
            SettingItem::FLock => "FLOCK",
            SettingItem::Info => "INFO",
            SettingItem::Reset => "RESET",
            SettingItem::AprsCall => "CALLSIGN",
            SettingItem::AprsFreq => "APRSFRQ",
            SettingItem::AprsLat => "APRSLAT",
            SettingItem::AprsLon => "APRSLON",
            SettingItem::AprsPath => "APRSPTH",
            SettingItem::AprsDevInfo => "APRDEV",
            SettingItem::AprsDevName => "APRDNAM",
            SettingItem::AprsBatVolt => "APRBAT",
            SettingItem::AprsComment => "APRCMSG",
            SettingItem::AprsSsid => "APRSSID",
            SettingItem::AprsSymbol => "APRSYM",
            SettingItem::AprsPower => "APRSPWR",
        }
    }

    pub fn is_placeholder(self) -> bool {
        false
    }

    pub fn is_scalar(self) -> bool {
        !matches!(
            self,
            SettingItem::Offse
                | SettingItem::BattCal
                | SettingItem::Info
                | SettingItem::Reset
                | SettingItem::AprsCall
                | SettingItem::AprsDevName
                | SettingItem::AprsComment
                | SettingItem::AprsFreq
                | SettingItem::AprsLat
                | SettingItem::AprsLon
        )
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

use super::input::DigitInput;
use super::name_edit::NameEdit;

pub(super) struct SettingsUi {
    /// `None` = top-level group selection; `Some(g)` = inside a group.
    pub(super) group: Option<SettingsGroup>,
    /// Item index within the current group, or group index at top level.
    pub(super) index: usize,
    pub(super) editing: bool,
    pub(super) snapshot: i32,
    pub(super) info_page: u8,
    pub(super) offset_input: DigitInput<7>,
    /// Digit entry for BATCAL: user types the multimeter-measured battery
    /// voltage (1 integer digit + 2 decimals, e.g. "742" = 7.42V) instead of
    /// adjusting the raw ADC calibration value directly with Up/Down.
    pub(super) battery_input: DigitInput<3>,
    /// T9 text editor for APRS callsign (max 6 chars).
    pub(super) aprs_call_edit: NameEdit<6>,
    /// T9 text editor for APRS device model name (max 6 chars).
    pub(super) aprs_dev_name_edit: NameEdit<6>,
    /// T9 text editor for APRS custom comment (max 16 chars).
    pub(super) aprs_comment_edit: NameEdit<16>,
    /// Digit entry for APRS TX frequency: 6 digits (XXX.XXX MHz).
    pub(super) aprs_freq_input: DigitInput<6>,
    /// Digit entry for APRS latitude: DDMM.mm
    pub(super) aprs_lat_input: DigitInput<6>,
    /// Digit entry for APRS longitude: DDDMM.mm
    pub(super) aprs_lon_input: DigitInput<7>,
    /// Hemisphere for latitude: false = N, true = S.
    pub(super) aprs_lat_neg: bool,
    /// Hemisphere for longitude: false = E, true = W.
    pub(super) aprs_lon_neg: bool,
}

impl SettingsUi {
    pub(super) const fn new() -> Self {
        SettingsUi {
            group: None,
            index: 0,
            editing: false,
            snapshot: 0,
            info_page: 0,
            offset_input: DigitInput::new(),
            battery_input: DigitInput::new(),
            aprs_call_edit: NameEdit::blank(),
            aprs_dev_name_edit: NameEdit::blank(),
            aprs_comment_edit: NameEdit::blank(),
            aprs_freq_input: DigitInput::new(),
            aprs_lat_input: DigitInput::new(),
            aprs_lon_input: DigitInput::new(),
            aprs_lat_neg: false,
            aprs_lon_neg: false,
        }
    }

    pub(super) fn is_editing(&self, index: usize) -> bool {
        self.editing && self.index == index
    }
}
