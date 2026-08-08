//! Readers for ROOT histogram classes.
//!
//! Currently one-dimensional histograms (`TH1D`, `TH1F`) and their `TAxis`.

use crate::rbytes::rbuffer::RBuffer;
use crate::rbytes::Error;

pub mod axis;
pub mod th1;

pub use axis::TAxis;
pub use th1::{BinType, Th1, H1D, H1F};

/// Read a `TArrayD`.
///
/// `TArray`'s streamer is hand-written in ROOT and emits neither a byte count nor a
/// version: the payload is just `fN:i32` followed by `fN` values. That is why these are
/// plain functions rather than [`crate::rbytes::Unmarshaler`] impls with a header.
pub(crate) fn read_array_f64(r: &mut RBuffer) -> crate::rbytes::Result<Vec<f64>> {
    let n = r.read_i32()?;
    if n < 0 {
        return Err(Error::Misc(format!("rhist: negative TArrayD length {n}")));
    }
    r.read_vec_f64(n as usize)
}

/// Read a `TArrayF`, widening every element to `f64`.
///
/// The widening is exact (every `f32` is representable as `f64`), so a `TH1F` read this
/// way compares bit-for-bit with what uproot reports after its own `float32 -> float64`
/// promotion.
pub(crate) fn read_array_f32(r: &mut RBuffer) -> crate::rbytes::Result<Vec<f64>> {
    let n = r.read_i32()?;
    if n < 0 {
        return Err(Error::Misc(format!("rhist: negative TArrayF length {n}")));
    }
    Ok(r
        .read_vec_f32(n as usize)?
        .into_iter()
        .map(|v| v as f64)
        .collect())
}
