//! Reader for [`TAxis`](https://root.cern/doc/master/classTAxis.html).

use crate::rbytes::rbuffer::RBuffer;
use crate::rbytes::Unmarshaler;
use crate::root::traits::Object;
use crate::{factory_fn_register_impl, rvers};
use log::trace;

/// `TAttAxis`: cosmetics only, read to keep the cursor aligned.
///
/// Kept private: nothing outside the histogram readers needs it, and its layout has been
/// stable since ROOT 3 (version 4).
#[derive(Debug, Clone, Default)]
pub(crate) struct AttAxis {
    pub(crate) ndivisions: i32,
    pub(crate) axis_color: i16,
    pub(crate) label_color: i16,
    pub(crate) label_font: i16,
    pub(crate) label_offset: f32,
    pub(crate) label_size: f32,
    pub(crate) tick_length: f32,
    pub(crate) title_offset: f32,
    pub(crate) title_size: f32,
    pub(crate) title_color: i16,
    pub(crate) title_font: i16,
}

impl Unmarshaler for AttAxis {
    fn unmarshal(&mut self, r: &mut RBuffer) -> crate::rbytes::Result<()> {
        let hdr = r.read_header("TAttAxis")?;
        trace!(";TAttAxis.unmarshal.vers:{}", hdr.vers);

        self.ndivisions = r.read_i32()?;
        self.axis_color = r.read_i16()?;
        self.label_color = r.read_i16()?;
        self.label_font = r.read_i16()?;
        self.label_offset = r.read_f32()?;
        self.label_size = r.read_f32()?;
        self.tick_length = r.read_f32()?;
        self.title_offset = r.read_f32()?;
        self.title_size = r.read_f32()?;
        self.title_color = r.read_i16()?;
        self.title_font = r.read_i16()?;

        let _ = rvers::ATT_AXIS;
        // Trailing members of a hypothetical newer TAttAxis are skipped by the byte count.
        r.set_pos_to_end_of(&hdr)?;
        Ok(())
    }
}

/// Rust equivalent of [`TAxis`](https://root.cern/doc/master/classTAxis.html).
///
/// Only the members needed to reconstruct the binning are exposed.
#[derive(Debug, Clone, Default)]
pub struct TAxis {
    pub(crate) name: String,
    pub(crate) title: String,
    pub(crate) att: AttAxis,
    /// Number of bins, excluding under- and overflow.
    pub(crate) n_bins: i32,
    pub(crate) xmin: f64,
    pub(crate) xmax: f64,
    /// Bin edges for a variable binning; empty when the binning is uniform.
    pub(crate) xbins: Vec<f64>,
    pub(crate) first: i32,
    pub(crate) last: i32,
    pub(crate) bits2: u16,
    pub(crate) time_display: bool,
    pub(crate) time_format: String,
}

impl TAxis {
    /// Number of bins, excluding under- and overflow.
    pub fn n_bins(&self) -> usize {
        self.n_bins.max(0) as usize
    }

    /// Lower edge of the axis (`fXmin`).
    pub fn low(&self) -> f64 {
        self.xmin
    }

    /// Upper edge of the axis (`fXmax`).
    pub fn high(&self) -> f64 {
        self.xmax
    }

    /// `true` when the axis carries an explicit `fXbins` array.
    pub fn is_variable_binning(&self) -> bool {
        !self.xbins.is_empty()
    }

    /// The `n_bins() + 1` bin edges.
    ///
    /// Uses `fXbins` when present (variable binning), otherwise interpolates between
    /// `fXmin` and `fXmax`, exactly like `TAxis::GetBinLowEdge`.
    pub fn edges(&self) -> Vec<f64> {
        let n = self.n_bins();
        if self.xbins.len() == n + 1 {
            return self.xbins.clone();
        }
        if n == 0 {
            return vec![self.xmin, self.xmax];
        }
        let w = (self.xmax - self.xmin) / n as f64;
        (0..=n).map(|i| self.xmin + w * i as f64).collect()
    }

    /// Bin centres, one per bin (under- and overflow excluded).
    pub fn centers(&self) -> Vec<f64> {
        let e = self.edges();
        e.windows(2).map(|w| 0.5 * (w[0] + w[1])).collect()
    }

    /// Name of the axis (`fName`, e.g. `"xaxis"`).
    pub fn axis_name(&self) -> &str {
        &self.name
    }
}

impl crate::root::traits::Named for TAxis {
    fn name(&self) -> &'_ str {
        &self.name
    }

    fn title(&self) -> &'_ str {
        &self.title
    }
}

impl Unmarshaler for TAxis {
    fn unmarshal(&mut self, r: &mut RBuffer) -> crate::rbytes::Result<()> {
        let hdr = r.read_header(self.class())?;
        trace!(";TAxis.unmarshal.vers:{}", hdr.vers);

        let named = r.read_object_into::<crate::rbase::Named>()?;
        self.name = named.name.clone();
        self.title = named.title.clone();

        self.att = r.read_object_into::<AttAxis>()?;

        self.n_bins = r.read_i32()?;
        self.xmin = r.read_f64()?;
        self.xmax = r.read_f64()?;
        self.xbins = super::read_array_f64(r)?;
        self.first = r.read_i32()?;
        self.last = r.read_i32()?;
        self.bits2 = r.read_u16()?;
        self.time_display = r.read_bool()?;
        self.time_format = r.read_string()?.to_string();

        // fLabels (THashList*) and, from TAxis v10 on, fModLabs (TList*). Both are
        // pointers we do not model; step over them using their own byte counts. The
        // final reposition below makes even that best-effort parse unnecessary, but
        // doing it keeps the cursor sane for headers without a byte count.
        r.skip_object_any()?;
        if hdr.vers >= rvers::AXIS {
            r.skip_object_any()?;
        }

        r.set_pos_to_end_of(&hdr)?;
        Ok(())
    }
}

factory_fn_register_impl!(TAxis, "TAxis");
