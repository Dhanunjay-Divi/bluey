//! Routing module aggregator.

pub mod dispatcher;

pub use dispatcher::{complete, resolve_route, Completion};
