//! The parsed datacard, still textual: shape files are not opened and wildcards
//! are not resolved. Turning this into something evaluable is `model`'s job.

use serde::Serialize;

/// One `shapes <process> <channel> <file> <hist> [<hist_syst>]` line. Several
/// may match a given (channel, process); the most specific wins.
#[derive(Debug, Clone, Serialize)]
pub struct ShapeLine {
    pub process: String,
    pub channel: String,
    /// `FAKE` for a counting card that has been promoted to shape form.
    pub file: String,
    pub hist_pattern: String,
    pub syst_pattern: Option<String>,
}

impl ShapeLine {
    pub fn is_fake(&self) -> bool {
        self.file == "FAKE"
    }

    /// How specific this line is. Higher wins when several lines match.
    pub fn specificity(&self) -> u8 {
        (self.process != "*") as u8 + (self.channel != "*") as u8
    }
}

/// The effect of one nuisance on one (channel, process) column.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Effect {
    /// A symmetric log-normal, `kappa`.
    Sym(f64),
    /// An asymmetric log-normal written `kappa_down/kappa_up`.
    Asym { down: f64, up: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SystKind {
    /// Log-normal. Gaussian constraint, theta in [-7, 7].
    LnN,
    /// Log-uniform: identical yield algebra to `lnN`, but a flat prior on [-1, 1].
    LnU,
    /// Vertical template morphing. `log` distinguishes `shapeN` from `shape`.
    Shape { log: bool },
    /// Template morphing with a uniform prior.
    ShapeU,
    /// Gamma (sideband) constraint: yield is `alpha * nu`, with `nu ~ Poisson(n_obs)`.
    GmN { n_obs: u32 },
    /// A parameter already present in the model, given a Gaussian prior.
    Param { mean: f64, sigma_lo: f64, sigma_hi: f64 },
    /// An unconstrained parameter that still floats in the fit.
    FlatParam,
    /// A free multiplicative factor on the matching (channel, process) yields.
    RateParam { channel: String, process: String, init: f64, range: Option<(f64, f64)> },
}

#[derive(Debug, Clone, Serialize)]
pub struct Systematic {
    pub name: String,
    /// Set by a `name[nofloat]` suffix: the parameter is frozen at its nominal.
    pub nofloat: bool,
    pub kind: SystKind,
    /// One entry per keyline column; `None` means no effect. Empty for the
    /// kinds carrying no column data (`param`, `flatParam`, `rateParam`).
    pub cols: Vec<Option<Effect>>,
    pub line: usize,
}

impl Systematic {
    /// Combine drops an all-null nuisance and decrements `kmax` accordingly.
    pub fn is_null(&self) -> bool {
        matches!(
            self.kind,
            SystKind::LnN | SystKind::LnU | SystKind::Shape { .. } | SystKind::ShapeU | SystKind::GmN { .. }
        ) && self.cols.iter().all(|c| c.is_none())
    }
}

/// An `autoMCStats` line: Barlow-Beeston(-lite) bin-wise MC statistical errors.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct AutoMcStats {
    /// Effective-entry count above which one bin-total (BB-lite) parameter is
    /// used instead of per-process ones. `-1` disables the nuisances entirely.
    pub threshold: f64,
    pub include_signal: bool,
    pub hist_mode: i32,
}

/// One (channel, process) column of the rate table.
#[derive(Debug, Clone, Serialize)]
pub struct KeyEntry {
    pub channel: String,
    pub process: String,
    /// From the second `process` row: `<= 0` is signal, `> 0` is background.
    pub is_signal: bool,
    /// `-1` means "take the integral from the input histogram".
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Datacard {
    pub imax: Option<usize>,
    pub jmax: Option<usize>,
    pub kmax: Option<usize>,
    pub shape_lines: Vec<ShapeLine>,
    pub channels: Vec<String>,
    /// Per channel. `-1` means "take from the input data object".
    pub observation: Vec<f64>,
    pub keyline: Vec<KeyEntry>,
    pub systs: Vec<Systematic>,
    /// Channel glob (`*` allowed) to settings.
    pub auto_mc_stats: Vec<(String, AutoMcStats)>,
}

impl Datacard {
    pub fn has_shapes(&self) -> bool {
        self.shape_lines.iter().any(|s| !s.is_fake())
    }

    /// The `shapes` line governing a (channel, process), most specific first.
    pub fn shape_line_for(&self, channel: &str, process: &str) -> Option<&ShapeLine> {
        self.shape_lines
            .iter()
            .filter(|s| {
                (s.channel == "*" || s.channel == channel)
                    && (s.process == "*" || s.process == process)
            })
            .max_by_key(|s| s.specificity())
    }
}
