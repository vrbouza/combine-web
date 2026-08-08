//! Readers for [`TH1`](https://root.cern/doc/master/classTH1.html) and its concrete
//! `TH1D` / `TH1F` flavours.
//!
//! On-disk layout of a `TH1D` (a `TH1F` is identical bar the trailing array's element
//! type), each nested object being prefixed by its own `[byte count][version]`:
//!
//! ```text
//! TH1D  = [hdr] TH1 [hdr] TArrayD(fArray)
//! TH1   = [hdr] TNamed TAttLine TAttFill TAttMarker
//!         fNcells:i32
//!         fXaxis:TAxis fYaxis:TAxis fZaxis:TAxis
//!         fBarOffset:i16 fBarWidth:i16
//!         fEntries fTsumw fTsumw2 fTsumwx fTsumwx2 fMaximum fMinimum fNormFactor : f64
//!         fContour:TArrayD fSumw2:TArrayD
//!         fOption:TString fFunctions:TList*
//!         fBufferSize:i32 fBuffer:double*         (v >= 4)
//!         fBinStatErrOpt:i32                      (v >= 7)
//!         fStatOverflows:i32                      (v >= 8)
//! ```
//!
//! Everything from `fOption` on is only parsed opportunistically: the reader then jumps
//! to the end of the `TH1` record using its byte count, so unknown trailing members of a
//! future `TH1` version cannot desynchronise the bin array that follows.

use crate::rbytes::rbuffer::RBuffer;
use crate::rbytes::Unmarshaler;
use crate::rhist::axis::TAxis;
use crate::root::traits::Object;
use crate::{factory_fn_register_impl, rvers};
use log::trace;

/// Element type of the trailing `TArray` holding the bin contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinType {
    /// `TH1D`, stored as `TArrayD`.
    #[default]
    F64,
    /// `TH1F`, stored as `TArrayF`.
    F32,
}

/// Rust equivalent of a one-dimensional ROOT histogram (`TH1D`, `TH1F`).
///
/// Bin indices follow the ROOT convention internally: index 0 is the underflow bin and
/// index `n_bins()+1` the overflow. The ergonomic accessors ([`values`](Th1::values),
/// [`errors`](Th1::errors)) hide that and return exactly `n_bins()` entries.
#[derive(Debug, Clone, Default)]
pub struct Th1 {
    pub(crate) class_name: String,
    pub(crate) name: String,
    pub(crate) title: String,

    /// `fNcells` = n_bins + 2.
    pub(crate) n_cells: i32,
    pub(crate) x_axis: TAxis,
    pub(crate) y_axis: TAxis,
    pub(crate) z_axis: TAxis,

    pub(crate) entries: f64,
    pub(crate) tsumw: f64,
    pub(crate) tsumw2: f64,
    pub(crate) tsumwx: f64,
    pub(crate) tsumwx2: f64,
    pub(crate) maximum: f64,
    pub(crate) minimum: f64,
    pub(crate) norm_factor: f64,

    pub(crate) contour: Vec<f64>,
    /// `fSumw2`; empty when `TH1::Sumw2()` was never called.
    pub(crate) sumw2: Vec<f64>,
    pub(crate) option: String,

    /// `fArray`, length `fNcells` (under/overflow included).
    pub(crate) data: Vec<f64>,
    pub(crate) bin_type: BinType,
    /// `TH1` streamer version actually found on disk.
    pub(crate) th1_version: i16,
}

impl Th1 {
    /// Class name as written in the file: `"TH1D"` or `"TH1F"`.
    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    /// `fName`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `fTitle`.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Number of bins, excluding under- and overflow.
    pub fn n_bins(&self) -> usize {
        self.x_axis.n_bins()
    }

    /// Lower edge of the x axis.
    pub fn low(&self) -> f64 {
        self.x_axis.low()
    }

    /// Upper edge of the x axis.
    pub fn high(&self) -> f64 {
        self.x_axis.high()
    }

    /// The `n_bins() + 1` bin edges, honouring a variable binning (`fXbins`).
    pub fn edges(&self) -> Vec<f64> {
        self.x_axis.edges()
    }

    /// Bin centres, one per bin.
    pub fn centers(&self) -> Vec<f64> {
        self.x_axis.centers()
    }

    /// The x axis.
    pub fn x_axis(&self) -> &TAxis {
        &self.x_axis
    }

    /// Bin contents for bins `1..=n_bins()`, i.e. under- and overflow excluded.
    pub fn values(&self) -> &[f64] {
        let n = self.n_bins();
        if self.data.len() >= n + 2 {
            &self.data[1..=n]
        } else {
            // Defensive: a corrupt fNcells should not panic.
            &self.data
        }
    }

    /// The raw `fArray`, under- and overflow included (`fNcells` entries).
    pub fn raw_values(&self) -> &[f64] {
        &self.data
    }

    /// Content of the underflow bin.
    pub fn underflow(&self) -> f64 {
        self.data.first().copied().unwrap_or(0.0)
    }

    /// Content of the overflow bin.
    pub fn overflow(&self) -> f64 {
        if self.data.len() >= self.n_bins() + 2 {
            self.data[self.n_bins() + 1]
        } else {
            0.0
        }
    }

    /// Raw `fSumw2` (under- and overflow included); empty when the histogram has none.
    pub fn sumw2(&self) -> &[f64] {
        &self.sumw2
    }

    /// Per-bin uncertainties for bins `1..=n_bins()`.
    ///
    /// Reproduces `TH1::GetBinError`:
    ///
    /// ```cpp
    /// if (fSumw2.fN) return TMath::Sqrt(fSumw2.fArray[bin]);
    /// return TMath::Sqrt(TMath::Abs(GetBinContent(bin)));
    /// ```
    ///
    /// Getting this wrong is silent — the numbers stay plausible — and corrupts every
    /// Barlow-Beeston / autoMCStats result downstream, so both branches are implemented.
    ///
    /// Of the two files used to validate this reader, `htt_input.root` exercises the
    /// `fSumw2` branch (`fSumw2` has `fNcells` entries, some of them legitimately 0) and
    /// `simple-shapes-TH1_input.root` exercises the `sqrt(|content|)` fallback.
    ///
    /// Two deliberate deviations from the C++:
    ///
    /// * ROOT only tests `fSumw2.fN != 0` and would then index past the end of a short
    ///   `fSumw2`; the length is required to cover all `fNcells` here, as uproot does.
    /// * For a *negative* bin content in the fallback branch, ROOT (and this method)
    ///   return `sqrt(|content|)` while uproot returns 0. ROOT's convention is the one
    ///   implemented, since it is what Combine sees when it runs against real ROOT.
    pub fn errors(&self) -> Vec<f64> {
        let n = self.n_bins();
        if self.sumw2.len() >= n + 2 {
            (1..=n).map(|i| self.sumw2[i].sqrt()).collect()
        } else {
            self.values().iter().map(|v| v.abs().sqrt()).collect()
        }
    }

    /// Sum of the bin contents over bins `1..=n_bins()` (under- and overflow excluded),
    /// i.e. `TH1::Integral()`.
    pub fn integral(&self) -> f64 {
        self.values().iter().sum()
    }

    /// `fEntries`.
    pub fn entries(&self) -> f64 {
        self.entries
    }

    /// `fTsumw`, `fTsumw2`, `fTsumwx`, `fTsumwx2`.
    pub fn stats(&self) -> (f64, f64, f64, f64) {
        (self.tsumw, self.tsumw2, self.tsumwx, self.tsumwx2)
    }

    /// `TH1` streamer version read from the file (6, 7, 8, ...).
    pub fn streamer_version(&self) -> i16 {
        self.th1_version
    }

    /// Parse the `TH1` base record. The caller is responsible for the trailing
    /// `TArrayD`/`TArrayF` holding the bin contents.
    fn unmarshal_th1(&mut self, r: &mut RBuffer) -> crate::rbytes::Result<()> {
        let hdr = r.read_header("TH1")?;
        let vers = hdr.vers;
        self.th1_version = vers;
        trace!(";TH1.unmarshal.vers:{}", vers);

        if vers < 3 {
            return Err(crate::rbytes::Error::VersionTooLow {
                class: "TH1".into(),
                version_read: vers,
                min_expected: 3,
            });
        }
        if vers > rvers::H1 {
            // Not fatal: everything up to fSumw2 has been stable since TH1 v3, and the
            // reposition at the end of this function absorbs any new trailing member.
            trace!(
                ";TH1.unmarshal.version_newer_than_known:{} > {}",
                vers,
                rvers::H1
            );
        }

        let named = r.read_object_into::<crate::rbase::Named>()?;
        self.name = named.name.clone();
        self.title = named.title.clone();

        let _att_line = r.read_object_into::<crate::rbase::AttLine>()?;
        let _att_fill = r.read_object_into::<crate::rbase::AttFill>()?;
        let _att_marker = r.read_object_into::<crate::rbase::AttMarker>()?;

        self.n_cells = r.read_i32()?;
        self.x_axis = r.read_object_into::<TAxis>()?;
        self.y_axis = r.read_object_into::<TAxis>()?;
        self.z_axis = r.read_object_into::<TAxis>()?;

        let _bar_offset = r.read_i16()?;
        let _bar_width = r.read_i16()?;

        self.entries = r.read_f64()?;
        self.tsumw = r.read_f64()?;
        self.tsumw2 = r.read_f64()?;
        self.tsumwx = r.read_f64()?;
        self.tsumwx2 = r.read_f64()?;
        self.maximum = r.read_f64()?;
        self.minimum = r.read_f64()?;
        self.norm_factor = r.read_f64()?;

        self.contour = super::read_array_f64(r)?;
        self.sumw2 = super::read_array_f64(r)?;
        self.option = r.read_string()?.to_string();

        // --- version-dependent tail -------------------------------------------------
        // fFunctions is a TList* that routinely holds TPaveStats / TF1 objects this
        // crate cannot instantiate, so it is stepped over rather than read. Everything
        // after it is parsed only to keep the cursor meaningful for buffers written
        // without a byte count; the reposition below is what actually guarantees
        // correctness.
        r.skip_object_any()?;

        if vers >= 4 {
            let buf_size = r.read_i32()?;
            let has_buffer = r.read_i8()?;
            if has_buffer != 0 && buf_size > 0 {
                let _buffer = r.read_vec_f64(buf_size as usize)?;
            }
        }
        if vers >= 7 {
            let _bin_stat_err_opt = r.read_i32()?;
        }
        if vers >= 8 {
            let _stat_overflows = r.read_i32()?;
        }

        r.set_pos_to_end_of(&hdr)?;
        Ok(())
    }

    /// Full `TH1D`/`TH1F` record: the `TH1` base followed by the bin array.
    fn unmarshal_hist(
        &mut self,
        r: &mut RBuffer,
        class: &str,
        max_vers: i16,
        bin_type: BinType,
    ) -> crate::rbytes::Result<()> {
        let hdr = r.read_header(class)?;
        trace!(";{}.unmarshal.vers:{}", class, hdr.vers);
        if hdr.vers > max_vers {
            trace!(
                ";{}.unmarshal.version_newer_than_known:{} > {}",
                class,
                hdr.vers,
                max_vers
            );
        }

        self.class_name = class.to_string();
        self.bin_type = bin_type;

        self.unmarshal_th1(r)?;

        self.data = match bin_type {
            BinType::F64 => super::read_array_f64(r)?,
            BinType::F32 => super::read_array_f32(r)?,
        };

        r.set_pos_to_end_of(&hdr)?;
        Ok(())
    }
}

impl crate::root::traits::Named for Th1 {
    fn name(&self) -> &'_ str {
        &self.name
    }

    fn title(&self) -> &'_ str {
        &self.title
    }
}

impl Object for Th1 {
    fn class(&self) -> &'_ str {
        &self.class_name
    }
}

macro_rules! impl_th1_flavour {
    ($t:ident, $class:literal, $vers:expr, $bin:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Default)]
        pub struct $t {
            pub(crate) hist: Th1,
        }

        impl $t {
            /// The parsed histogram.
            pub fn hist(&self) -> &Th1 {
                &self.hist
            }

            /// Consume the wrapper and yield the histogram.
            pub fn into_hist(self) -> Th1 {
                self.hist
            }
        }

        impl crate::root::traits::Named for $t {
            fn name(&self) -> &'_ str {
                &self.hist.name
            }

            fn title(&self) -> &'_ str {
                &self.hist.title
            }
        }

        impl Unmarshaler for $t {
            fn unmarshal(&mut self, r: &mut RBuffer) -> crate::rbytes::Result<()> {
                self.hist.unmarshal_hist(r, $class, $vers, $bin)
            }
        }

        factory_fn_register_impl!($t, $class);
    };
}

impl_th1_flavour!(
    H1D,
    "TH1D",
    rvers::H1D,
    BinType::F64,
    "Reader for `TH1D` (bin contents stored as `TArrayD`)."
);
impl_th1_flavour!(
    H1F,
    "TH1F",
    rvers::H1F,
    BinType::F32,
    "Reader for `TH1F` (bin contents stored as `TArrayF`, widened to `f64` on read)."
);
