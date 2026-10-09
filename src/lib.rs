//! Local-only notebook data, durable storage and portable native snapshots.
#[cfg(feature = "desktop")]
pub mod app;
#[cfg(feature = "desktop")]
mod fonts;
pub mod import_export;
pub mod model;
pub mod nebula_format;
pub mod storage;
#[cfg(feature = "desktop")]
mod theme;
