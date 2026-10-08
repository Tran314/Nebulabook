//! Local-only notebook data, durable storage and non-destructive migration.
#[cfg(feature = "desktop")]
pub mod app;
#[cfg(feature = "desktop")]
mod fonts;
pub mod import_export;
pub mod model;
pub mod runtime;
pub mod storage;
