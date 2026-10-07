//! Routes and final settings for the original desktop onboarding branches.
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    Welcome,
    TrackerType,
    Wifi,
    Connect,
    Dongle,
    Assignment,
    Mounting,
    Height,
    Usage,
    Runtime,
    Mocap,
}
impl Step {
    pub fn progress(self) -> f32 {
        match self {
            Self::Welcome => 0.1,
            Self::TrackerType | Self::Wifi => 0.2,
            Self::Connect => 0.4,
            Self::Dongle | Self::Assignment => 0.5,
            Self::Mounting => 0.6,
            Self::Height => 0.7,
            Self::Usage => 0.8,
            Self::Runtime | Self::Mocap => 0.9,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackerSet {
    Regular,
    Butterfly,
    Wifi,
    Dongle,
}
impl TrackerSet {
    pub fn connection(self) -> Step {
        match self {
            Self::Regular | Self::Wifi => Step::Wifi,
            Self::Butterfly | Self::Dongle => Step::Dongle,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Skip,
    MoreSets,
    WifiError(u64),
}
pub struct Setup {
    pub step: Step,
    pub tracker_set: Option<TrackerSet>,
    pub mocap: bool,
    pub usage_selected: bool,
    pub standalone: bool,
    pub head_tracker: bool,
    pub forehead: Option<bool>,
    pub standing: Option<bool>,
    pub dialog: Option<Dialog>,
    pub wifi_request: Option<(u64, u32, u64)>,
    pub wifi_status: u64,
    pub serial_return: bool,
}
impl Default for Setup {
    fn default() -> Self {
        Self {
            step: Step::Welcome,
            tracker_set: None,
            mocap: false,
            usage_selected: false,
            standalone: false,
            head_tracker: true,
            forehead: None,
            standing: None,
            dialog: None,
            wifi_request: None,
            wifi_status: 0,
            serial_return: false,
        }
    }
}
impl Setup {
    pub fn accepts_wifi(&self, session: u64, tx: u32, sequence: u64) -> bool {
        self.wifi_request.is_some_and(|(active, request, after)| {
            active == session && sequence > after && (tx == request || tx == 0)
        })
    }
    pub fn can_finish_mocap(&self) -> bool {
        !self.head_tracker || (self.forehead.is_some() && self.standing.is_some())
    }
    pub fn settings(&self, current: &Value) -> Value {
        let mut toggles = current["model_settings"]["toggles"].clone();
        let mut resets = current["resets_settings"].clone();
        let mut osc = current["vrc_osc"]["osc_settings"].clone();
        if !toggles.is_object() {
            toggles = json!({});
        }
        if !resets.is_object() {
            resets = json!({});
        }
        if !osc.is_object() {
            osc = json!({});
        }
        toggles["self_localization"] = json!(self.mocap && self.standing == Some(true));
        resets["reset_hmd_pitch"] = json!(self.head_tracker && self.forehead == Some(true));
        osc["enabled"] = json!(!self.mocap && self.standalone);
        json!({"model_settings":{"toggles":toggles},"resets_settings":resets,"vrc_osc":{"osc_settings":osc}})
    }
}
