use super::Mode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LauncherEntry {
    Settings,
    ChannelMgr,
    Contacts,
    FmRadio,
    ScanQt,
    Search,
    Spectrum,
}

pub const LAUNCHER_ITEMS: &[LauncherEntry] = &[
    LauncherEntry::Settings,
    LauncherEntry::ChannelMgr,
    LauncherEntry::Contacts,
    LauncherEntry::FmRadio,
    LauncherEntry::ScanQt,
    LauncherEntry::Search,
    LauncherEntry::Spectrum,
];

impl LauncherEntry {
    pub fn label(self) -> &'static str {
        match self {
            LauncherEntry::Settings => "SETTINGS",
            LauncherEntry::ChannelMgr => "CHANNELS",
            LauncherEntry::Contacts => "CONTACTS",
            LauncherEntry::FmRadio => "FM RADIO",
            LauncherEntry::ScanQt => "QT SCAN",
            LauncherEntry::Search => "FREQ HUNT",
            LauncherEntry::Spectrum => "SPECTRUM",
        }
    }

    pub fn is_available(self) -> bool {
        match self {
            LauncherEntry::Settings => true,
            LauncherEntry::ChannelMgr => true,
            LauncherEntry::Contacts => true,
            LauncherEntry::FmRadio => true,
            LauncherEntry::ScanQt => true,
            LauncherEntry::Search => true,
            LauncherEntry::Spectrum => true,
        }
    }

    pub fn target_mode(self) -> Mode {
        match self {
            LauncherEntry::Settings => Mode::Settings,
            LauncherEntry::ChannelMgr => Mode::ChanMgr,
            LauncherEntry::Contacts => Mode::Contacts,
            LauncherEntry::FmRadio => Mode::Fm,
            LauncherEntry::ScanQt => Mode::ScanQt,
            LauncherEntry::Search => Mode::Search,
            LauncherEntry::Spectrum => Mode::Spectrum,
        }
    }
}
