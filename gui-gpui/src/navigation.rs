//! Original sidebar order and return location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    SteamVr,
    StayAligned,
    Mechanics,
    Fk,
    Gestures,
    Notifications,
    Behavior,
    Appearance,
    Home,
    Checklist,
    OscRouter,
    OscVrchat,
    OscVmc,
    Serial,
    Firmware,
    Onboarding,
    Advanced,
}
impl Section {
    pub const GROUPS: &'static [(&'static str, &'static [Self])] = &[
        (
            "settings-sidebar-general",
            &[
                Self::SteamVr,
                Self::StayAligned,
                Self::Mechanics,
                Self::Fk,
                Self::Gestures,
            ],
        ),
        (
            "settings-sidebar-interface",
            &[
                Self::Notifications,
                Self::Behavior,
                Self::Appearance,
                Self::Home,
                Self::Checklist,
            ],
        ),
        ("OSC", &[Self::OscRouter, Self::OscVrchat, Self::OscVmc]),
        (
            "settings-sidebar-utils",
            &[
                Self::Serial,
                Self::Firmware,
                Self::Onboarding,
                Self::Advanced,
            ],
        ),
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::SteamVr => "settings-sidebar-steamvr",
            Self::StayAligned => "settings-sidebar-stay_aligned",
            Self::Mechanics => "settings-sidebar-tracker_mechanics",
            Self::Fk => "settings-sidebar-fk_settings",
            Self::Gestures => "settings-sidebar-gesture_control",
            Self::Notifications => "settings-sidebar-notifications",
            Self::Behavior => "settings-sidebar-behavior",
            Self::Appearance => "settings-sidebar-appearance",
            Self::Home => "settings-sidebar-home",
            Self::Checklist => "settings-sidebar-checklist",
            Self::OscRouter => "settings-sidebar-osc_router",
            Self::OscVrchat => "settings-sidebar-osc_trackers",
            Self::OscVmc => "settings-sidebar-osc_vmc",
            Self::Serial => "settings-sidebar-serial",
            Self::Firmware => "settings-sidebar-firmware-tool",
            Self::Onboarding => "navbar-onboarding",
            Self::Advanced => "settings-sidebar-advanced",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    #[default]
    Home,
    Assignment,
    Mounting,
    Proportions,
    Connect,
    Settings(Section),
    VrchatWarnings,
    Tracker(crate::protocol::TrackerKey),
    Checklist,
    VrMode,
    StayAlignedSetup,
}
impl Page {
    pub const SIDEBAR: [Self; 6] = [
        Self::Home,
        Self::Assignment,
        Self::Mounting,
        Self::Proportions,
        Self::Connect,
        Self::Settings(Section::SteamVr),
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Home => "navbar-home",
            Self::Assignment => "navbar-trackers_assign",
            Self::Mounting => "navbar-mounting",
            Self::Proportions => "navbar-body_proportions",
            Self::Connect => "navbar-connect_trackers",
            Self::Settings(s) => s.label(),
            Self::VrchatWarnings => "settings-sidebar-vrc_warnings",
            Self::Tracker(_) => "native-tracker-settings",
            Self::Checklist => "native-checklist",
            Self::VrMode => "vrmode-title",
            Self::StayAlignedSetup => "onboarding-stay_aligned-title",
        }
    }
    pub fn in_settings(self) -> bool {
        matches!(self, Self::Settings(_) | Self::VrchatWarnings)
    }
}

#[derive(Default)]
pub struct Navigation {
    pub page: Page,
    warning_origin: Option<Page>,
}
impl Navigation {
    pub fn go(&mut self, page: Page) {
        if page == Page::VrchatWarnings && self.page != page {
            self.warning_origin = Some(self.page);
        }
        self.page = page;
    }
    pub fn back_from_warning(&mut self) {
        self.page = self
            .warning_origin
            .take()
            .unwrap_or(Page::Settings(Section::SteamVr));
    }
}
