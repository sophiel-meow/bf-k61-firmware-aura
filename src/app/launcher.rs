use super::Mode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LauncherEntry {
    Settings,
    ChannelMgr,
    Contacts,
    FmRadio,
    ScanQt,
    Search,
}

pub const LAUNCHER_ITEMS: &[LauncherEntry] = &[
    LauncherEntry::Settings,
    LauncherEntry::ChannelMgr,
    LauncherEntry::Contacts,
    LauncherEntry::FmRadio,
    LauncherEntry::ScanQt,
    LauncherEntry::Search,
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
        }
    }
}
