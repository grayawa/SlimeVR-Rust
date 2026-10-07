//! OSC socket ownership is independent of the deterministic pose owner.
pub mod armature;
pub mod config;
pub mod state;
pub mod worker;
pub use config::Settings;
pub use state::State;
pub use worker::{Controller, Event};
pub mod query;
