//! Presentation policy shared by the desktop home and SteamVR dashboard.
use crate::protocol::{Feed, Tracker};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub table: bool,
    pub sort_by_name: bool,
    pub include_computed: bool,
    pub more_info: bool,
}
impl Settings {
    pub fn from_preferences(value: &Value) -> Self {
        Self {
            table: value["homeLayout"] == "table",
            sort_by_name: value["devSettings"]["sortByName"] == true,
            include_computed: value["debug"] == true
                && value["devSettings"]["filterSlimesAndHMD"] != true,
            more_info: value["devSettings"]["moreInfo"] == true,
        }
    }
    pub fn groups<'a>(&self, feed: &'a Feed) -> [Vec<&'a Tracker>; 2] {
        let mut groups = [Vec::new(), Vec::new()];
        for tracker in &feed.trackers {
            if !tracker.computed || self.include_computed {
                groups[usize::from(tracker.body == 0)].push(tracker);
            }
        }
        if self.sort_by_name {
            for group in &mut groups {
                group.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }
        groups
    }
}
