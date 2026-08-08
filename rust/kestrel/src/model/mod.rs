//! The compiled model: a flat, precomputed form of the datacard that the
//! likelihood evaluates against. Every `ln kappa` and per-bin template delta is
//! computed once here so the hot loop does no lookups and no string work.

mod compile;
mod params;

pub use params::{Param, ParamKind};
// `ShapeHandling` is declared below, next to `CompiledModel`.

use std::ops::Range;

/// Combine's `smoothStepFunc`: `(3u^5 - 10u^3 + 15u)/8`, which is +-1 with zero
/// first and second derivatives at `u = +-1` (`interface/CombineMathFuncs.h`).
#[inline]
pub fn smooth_step(x: f64, smooth_region: f64) -> f64 {
    if x.abs() >= smooth_region {
        return if x > 0.0 { 1.0 } else { -1.0 };
    }
    let u = x / smooth_region;
    let u2 = u * u;
    0.125 * u * (u2 * (3.0 * u2 - 10.0) + 15.0)
}

/// Combine's `logKappaForX`, the asymmetric log-normal interpolator. Outside
/// |theta| >= 0.5 it is the plain one-sided log-kappa.
#[inline]
pub fn log_kappa_for_x(theta: f64, log_k_lo: f64, log_k_hi: f64) -> f64 {
    if theta.abs() >= 0.5 {
        return if theta >= 0.0 { log_k_hi } else { -log_k_lo };
    }
    let (hi, lo) = (log_k_hi, -log_k_lo);
    let (avg, halfdiff) = (0.5 * (hi + lo), 0.5 * (hi - lo));
    let twox = theta + theta;
    let twox2 = twox * twox;
    let alpha = 0.125 * twox * (twox2 * (3.0 * twox2 - 10.0) + 15.0);
    avg + alpha * halfdiff
}

/// A free multiplicative factor on a process yield: `rateParam`, and `gmN`
/// (where `scale` is `1/n_obs`, so the factor is `nu / n_obs`).
#[derive(Debug, Clone)]
pub struct Factor {
    pub param: usize,
    pub scale: f64,
}

#[derive(Debug, Clone)]
pub struct Morph {
    pub param: usize,
    /// `shapeN` interpolates the log of the fraction rather than the fraction.
    pub log_space: bool,
    /// The datacard column value: theta is scaled by it before morphing.
    pub scale: f64,
    /// Offset into `morph_sum`/`morph_diff`; length is the owning process's bins.
    pub off: usize,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub bin_off: usize,
    pub n_bins: usize,
    pub procs: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Proc {
    pub channel: usize,
    pub name: String,
    pub is_signal: bool,
    pub nominal: f64,
    /// Offset into `templates`; length is the channel's bin count.
    pub tmpl_off: usize,
    pub log_kappa: Vec<(usize, f64)>,
    pub asym_kappa: Vec<(usize, f64, f64)>,
    pub factors: Vec<Factor>,
    pub morphs: Range<usize>,
    /// `qrange`: the smallest epsilon over this process's shape columns.
    pub smooth_region: f64,
}

#[derive(Debug, Clone)]
pub struct CompiledModel {
    pub params: Vec<Param>,
    pub poi: usize,
    pub channels: Vec<Channel>,
    pub procs: Vec<Proc>,
    pub morphs: Vec<Morph>,
    /// Nominal bin fractions, normalised to unity per process.
    pub templates: Vec<f64>,
    /// Per-bin `delta_up + delta_down` and `delta_up - delta_down`, precomputed
    /// on the unit-normalised fractions as Combine does.
    pub morph_sum: Vec<f64>,
    pub morph_diff: Vec<f64>,
    /// Per-bin MC statistical errors as a fraction of the process integral,
    /// laid out exactly like `templates`.
    pub errors: Vec<f64>,
    pub obs: Vec<f64>,
    pub n_bins: usize,
    /// Global bin indices carrying a Barlow-Beeston-lite parameter. Combine
    /// solves these analytically each call rather than giving them to Migrad.
    pub bb_bins: Vec<usize>,
    /// Non-empty only under `ShapeHandling::Ignore`; surfaced to the user.
    pub ignored_shape_systs: Vec<String>,
    /// Caveats the interface must show. Not errors: the fit runs, but the
    /// result is not reference-grade.
    pub warnings: Vec<String>,
}

/// Reusable buffers for one likelihood evaluation.
#[derive(Debug, Default, Clone)]
pub struct EvalScratch {
    frac: Vec<f64>,
    /// Coefficient-weighted sum of squared errors, per bin.
    err2: Vec<f64>,
    /// The solved Barlow-Beeston parameters, parallel to `bb_bins`.
    pub bb_x: Vec<f64>,
}

/// What to do with `shape`/`shapeN` systematics.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ShapeHandling {
    /// Full vertical template morphing, as Combine does.
    #[default]
    Morph,
    /// Skip them entirely: no parameters, no Up/Down templates read. The fit is
    /// then less constrained than Combine's, so callers must warn.
    Ignore,
}

impl CompiledModel {
    /// Per-process normalisation `N_p = nominal * exp(sum) * prod(factors)`.
    /// The `exp` is hoisted out of the bin loop; on a counting card this is the
    /// entire cost of an evaluation.
    #[inline]
    pub fn normalisation(&self, p: &Proc, par: &[f64]) -> f64 {
        let mut log_val = 0.0;
        for &(i, lnk) in &p.log_kappa {
            log_val += par[i] * lnk;
        }
        for &(i, lo, hi) in &p.asym_kappa {
            let x = par[i];
            log_val += x * log_kappa_for_x(x, lo, hi);
        }
        let mut n = p.nominal * log_val.exp();
        for f in &p.factors {
            n *= par[f.param] * f.scale;
        }
        n
    }

    /// Expected yields for every bin, written into `out` (length `n_bins`).
    ///
    /// With `autoMCStats` the Barlow-Beeston parameters are solved here rather
    /// than handed to Migrad, matching Combine's default; the solved values are
    /// left in `s.bb_x` so the likelihood can add their constraint.
    pub fn expected(&self, par: &[f64], out: &mut [f64], s: &mut EvalScratch) {
        out.fill(0.0);
        let bb = !self.bb_bins.is_empty();
        if bb {
            s.err2.clear();
            s.err2.resize(self.n_bins, 0.0);
        }
        let r = par[self.poi];
        for ch in &self.channels {
            for p in &self.procs[ch.procs.clone()] {
                let mult = if p.is_signal { r } else { 1.0 };
                let n = mult * self.normalisation(p, par);
                let tmpl = &self.templates[p.tmpl_off..p.tmpl_off + ch.n_bins];
                let dst = &mut out[ch.bin_off..ch.bin_off + ch.n_bins];

                if bb {
                    // Combine weights the errors by the same coefficient as the
                    // values, so this is r-dependent for signal.
                    let err = &self.errors[p.tmpl_off..p.tmpl_off + ch.n_bins];
                    let e2 = &mut s.err2[ch.bin_off..ch.bin_off + ch.n_bins];
                    for (acc, e) in e2.iter_mut().zip(err) {
                        let v = n * e;
                        *acc += v * v;
                    }
                }

                if p.morphs.is_empty() {
                    for (o, t) in dst.iter_mut().zip(tmpl) {
                        *o += n * t;
                    }
                    continue;
                }

                let scratch = &mut s.frac;
                scratch.clear();
                scratch.extend_from_slice(tmpl);
                for m in &self.morphs[p.morphs.clone()] {
                    let x = m.scale * par[m.param];
                    let a = 0.5 * x;
                    let b = smooth_step(x, p.smooth_region);
                    let sum = &self.morph_sum[m.off..m.off + ch.n_bins];
                    let diff = &self.morph_diff[m.off..m.off + ch.n_bins];
                    for k in 0..ch.n_bins {
                        let alpha = a * (diff[k] + b * sum[k]);
                        if m.log_space {
                            scratch[k] *= alpha.exp();
                        } else {
                            scratch[k] += alpha;
                        }
                    }
                }
                // Combine crops at 1e-9 and renormalises the fractions.
                let mut tot = 0.0;
                for v in scratch.iter_mut() {
                    *v = v.max(1e-9);
                    tot += *v;
                }
                let inv = if tot > 0.0 { 1.0 / tot } else { 0.0 };
                for (o, v) in dst.iter_mut().zip(scratch.iter()) {
                    *o += n * v * inv;
                }
            }
        }

        // `KESTREL_NO_BB` leaves the Barlow-Beeston parameters at zero, which is
        // the state Combine's `generateBinned` reports — the only way to compare
        // the underlying model against the oracle on an autoMCStats card.
        if bb && !cfg!(target_family = "wasm") && std::env::var_os("KESTREL_NO_BB").is_none() {
            self.solve_barlow_beeston(out, s);
        }
    }

    /// The stationary point of `(v + sigma x) - d ln(v + sigma x) + (x - g)^2/2`,
    /// via the numerically stable root of `x^2 + bx + c = 0`
    /// (`CMSHistErrorPropagator.cc:192-214`). The global observable `g` is zero.
    fn solve_barlow_beeston(&self, out: &mut [f64], s: &mut EvalScratch) {
        s.bb_x.clear();
        for &j in &self.bb_bins {
            let (valsum, toterr) = (out[j], s.err2[j].sqrt());
            if !(toterr > 0.0) {
                s.bb_x.push(0.0);
                continue;
            }
            let ratio = valsum / toterr;
            let b = toterr + ratio;
            let c = valsum - self.obs[j];
            let disc = b * b - 4.0 * c;
            let x = if disc < 0.0 {
                0.0
            } else {
                let tmp = -0.5 * (b + b.signum() * disc.sqrt());
                if tmp == 0.0 { 0.0 } else { tmp.max(c / tmp) }
            };
            out[j] = valsum + toterr * x;
            s.bb_x.push(x);
        }
    }
}
