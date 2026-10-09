//! Bounded local SolarXR API. Commands execute on the receiver / pose owner's clock.
//! Transport owns connection subscriptions; Service owns application state.
mod config;
pub mod device_control;
pub mod diagnostics;
pub mod live;
pub mod protocol;
pub mod pubsub;
mod service;
mod settings;
mod shared_config;
mod status;
mod transport;
mod types;
pub mod vrchat;

pub use config::FrontendConfig;
pub use service::Service;
pub use shared_config::SharedConfig;
pub use transport::serve;
pub(crate) use types::error_wire;
pub use types::{LiveState, Request, Wire};
