//! Upstream provider dispatcher. Stub for now; full proxying
//! implementation in subsequent commits.

pub struct Dispatcher;

impl Dispatcher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
