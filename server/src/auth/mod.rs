//! Auth module aggregator.

pub mod jwt;
pub mod middleware;
pub mod password;

pub use middleware::{require_admin, require_auth, AuthedAccount};
