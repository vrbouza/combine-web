//! CMS Combine-style binned maximum-likelihood fits, in pure Rust.
//!
//! Reads a datacard (and, for shape analyses, the TH1 templates it points at),
//! compiles it into a flat evaluation model, and minimises the negative
//! log-likelihood with a Rust transliteration of ROOT's Minuit2.
//!
//! This is a re-implementation validated against Combine's own reference
//! outputs — not a port of Combine, and not a wrapper around it.

pub mod datacard;
pub mod error;
pub mod fit;
pub mod impacts;
pub mod model;
pub mod nll;
pub mod report;
pub mod scan;
pub mod shapes;

pub use datacard::Datacard;
pub use error::{Error, Result};
pub use fit::{FitConfig, FitResult};
pub use impacts::{impacts, impacts_for, Anchor, Impact};
pub use model::{CompiledModel, ShapeHandling};
pub use nll::Nll;
pub use report::{report, Pull, Report};
pub use scan::{scan, scan_param, ScanPoint, ScanResult};
pub use shapes::{ShapeFiles, Templates};

#[cfg(target_family = "wasm")]
pub mod wasm;
