//! Session export functionality.
//! The actual implementation lives in db::search (same impl block on Database).
//! This module re-exports the options type for external consumers.

pub use crate::db::search::ExportOptions;
