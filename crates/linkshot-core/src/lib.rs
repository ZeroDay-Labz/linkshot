//! linkshot-core: the document model and file formats.
//!
//! Everything here is independent of any GUI or platform. Key names, enum
//! spellings and file layouts are stable so that `.snagx`, `.snagtheme`,
//! `.snagpalette`, `.snagt` and `.snagpresets` files round-trip losslessly.

pub mod color;
pub mod document;
pub mod enums;
pub mod geometry;
pub mod library;
pub mod naming;
pub mod presets;
pub mod rtf;
pub mod settings;
pub mod snagx;
pub mod style;
pub mod template;
pub mod undo;

pub use color::Argb;
pub use document::{CaptureObject, Document, Page};
pub use geometry::{Pt, Rect, Size};

/// Software version string written into documents we create.
pub const SOFTWARE_VERSION: &str = concat!("linkshot ", env!("CARGO_PKG_VERSION"));

/// Default canvas DPI on Windows-style documents.
pub const DEFAULT_DPI: f64 = 96.0;
