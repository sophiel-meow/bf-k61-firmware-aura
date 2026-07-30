use super::Mode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LauncherEntry {
    Settings,
    FmRadio,
    Scan,
    ScanQt,
}

pub const LAUNCHER_ITEMS: &[LauncherEntry] = &[
    LauncherEntry::Settings,
    LauncherEntry::FmRadio,
    LauncherEntry::Scan,
    LauncherEntry::ScanQt,
];

impl LauncherEntry {
    pub fn label(self) -> &'static str {
        match self {
            LauncherEntry::Settings => "SETTINGS",
            LauncherEntry::FmRadio => "FM RADIO",
            LauncherEntry::Scan => "SCAN",
            LauncherEntry::ScanQt => "QT SCAN",
        }
    }

    pub fn is_available(self) -> bool {
        match self {
            LauncherEntry::Settings => true,
            LauncherEntry::FmRadio => false,
            LauncherEntry::Scan => false,
            LauncherEntry::ScanQt => true,
        }
    }

    pub fn target_mode(self) -> Mode {
        match self {
            LauncherEntry::Settings => Mode::Settings,
            LauncherEntry::FmRadio => Mode::Fm,
            LauncherEntry::Scan => Mode::Scan,
            LauncherEntry::ScanQt => Mode::ScanQt,
        }
    }
}
