/// Fixed display order for the settings menu. Every item the original
/// firmware's menu covers that we've deliberately chosen not to build at
/// all (channel-table editing/rename/delete -- left to the CPS PC tool --
/// scan, multi-language, voice prompts) has no entry here at all, per an
/// explicit scope decision; it's not merely hidden.
///
/// Items backed by something with no real effect yet (no DCS driver, no
/// tone-burst primitive, no VOX/idle-lock timers) still get an entry, so
/// the menu's shape doesn't shift out from under the user as those land --
/// but `is_placeholder()` marks them non-editable until they do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Sql,
    Step,
    Tot,
    Tdr,
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
    RxDcs,
    TxDcs,
    Rtone,
    Rptrl,
    Bright,
    Roge,
    Info,
    Reset,
}

pub const MENU_ORDER: [MenuItem; 24] = [
    MenuItem::Sql,
    MenuItem::Step,
    MenuItem::Tot,
    MenuItem::Tdr,
    MenuItem::BusyLock,
    MenuItem::TxForbid,
    MenuItem::Wn,
    MenuItem::TxPr,
    MenuItem::RxCts,
    MenuItem::TxCts,
    MenuItem::Sftd,
    MenuItem::Offse,
    MenuItem::Beep,
    MenuItem::AutoLk,
    MenuItem::Vox,
    MenuItem::VoxLv,
    MenuItem::RxDcs,
    MenuItem::TxDcs,
    MenuItem::Rtone,
    MenuItem::Rptrl,
    MenuItem::Bright,
    MenuItem::Roge,
    MenuItem::Info,
    MenuItem::Reset,
];

impl MenuItem {
    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Sql => "SQL",
            MenuItem::Step => "STEP",
            MenuItem::Tot => "TOT",
            MenuItem::Tdr => "TDR",
            MenuItem::BusyLock => "BCL",
            MenuItem::TxForbid => "TXINH",
            MenuItem::Wn => "W/N",
            MenuItem::TxPr => "PWR",
            MenuItem::RxCts => "R-CTC",
            MenuItem::TxCts => "T-CTC",
            MenuItem::Sftd => "SHIFT",
            MenuItem::Offse => "OFFSET",
            MenuItem::Beep => "BEEP",
            MenuItem::AutoLk => "AUTOLK",
            MenuItem::Vox => "VOX",
            MenuItem::VoxLv => "VOXLV",
            MenuItem::RxDcs => "R-DCS",
            MenuItem::TxDcs => "T-DCS",
            MenuItem::Rtone => "RTONE",
            MenuItem::Rptrl => "RPTRL",
            MenuItem::Bright => "CONTR",
            MenuItem::Roge => "ROGER",
            MenuItem::Info => "INFO",
            MenuItem::Reset => "RESET",
        }
    }

    /// Items with no consuming logic yet -- selecting them wouldn't
    /// actually do anything, so they're shown but not enterable.
    pub fn is_placeholder(self) -> bool {
        matches!(
            self,
            MenuItem::AutoLk
                | MenuItem::Vox
                | MenuItem::VoxLv
                | MenuItem::RxDcs
                | MenuItem::TxDcs
                | MenuItem::Rtone
                | MenuItem::Rptrl
                | MenuItem::Bright
        )
    }
}

/// Standard CTCSS tones, tenths of Hz (67.0Hz..254.1Hz), for RXCTS/TXCTS to
/// step through. Index `usize::MAX` (via `Option::None`) means "off".
pub const CTCSS_TABLE: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148,
    1188, 1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738,
    1773, 1799, 1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336,
    2418, 2503, 2541,
];

/// `None` = off; `Some(i)` = index into `CTCSS_TABLE`.
pub fn ctcss_index(tenths_hz: Option<u16>) -> Option<usize> {
    tenths_hz.and_then(|hz| CTCSS_TABLE.iter().position(|&t| t == hz))
}
