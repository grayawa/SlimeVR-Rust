//! Immutable published configurations; copy only when the owner changes settings.
use super::FrontendConfig;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

// Deliberately no Clone: existing config.clone() calls create editable candidates
// via Deref. Published readers must use snapshot() to share the immutable Arc.
pub struct SharedConfig(Arc<FrontendConfig>);
impl SharedConfig {
    pub fn snapshot(&self) -> Arc<FrontendConfig> {
        self.0.clone()
    }
    pub(super) fn replace(&mut self, config: FrontendConfig) {
        self.0 = Arc::new(config);
    }
}
impl From<FrontendConfig> for SharedConfig {
    fn from(config: FrontendConfig) -> Self {
        Self(Arc::new(config))
    }
}
impl Deref for SharedConfig {
    type Target = FrontendConfig;
    fn deref(&self) -> &FrontendConfig {
        &self.0
    }
}
impl DerefMut for SharedConfig {
    fn deref_mut(&mut self) -> &mut FrontendConfig {
        Arc::make_mut(&mut self.0)
    }
}
