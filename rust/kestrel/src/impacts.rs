//! Nuisance impacts: Combine's `combineTool.py -M Impacts`.
//!
//! The impact of a nuisance on the POI is the shift the POI takes when that
//! nuisance is fixed at its post-fit +-1 sigma and everything else, the POI
//! included, is re-profiled. It measures the correlation between the two; it is
//! not a variance decomposition, and the shifts do not add in quadrature to the
//! total uncertainty.

use minuit2::{MnHesse, MnMinos};
use serde::{Deserialize, Serialize};

use crate::fit::{FitConfig, FitResult};
use crate::model::{CompiledModel, ParamKind};
use crate::nll::Nll;
use crate::report::prior_band;

/// One nuisance's impact, in the triplet layout of Combine's `impacts.json` so
/// that the two can be diffed directly and `plotImpacts.py` can read ours.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Impact {
    pub name: String,
    /// Combine's constraint label, which colours the name in the plot.
    pub kind: String,
    /// `[centre - w_lo, centre, centre + w_hi]` from the prior alone.
    pub prefit: [f64; 3],
    /// `[theta_hat - sigma_lo, theta_hat, theta_hat + sigma_hi]`.
    pub fit: [f64; 3],
    /// `[r(theta-), r_hat, r(theta+)]`.
    pub poi: [f64; 3],
    /// `max(|impact+|, |impact-|)`, which is Combine's sort key.
    pub impact: f64,
    /// False when the crossing search or one of the two re-fits failed. The row
    /// is still returned so the plot can mark it, rather than vanishing.
    pub ok: bool,
}

/// The global-fit state every per-nuisance fit starts from. Taking it as an
/// argument rather than recomputing it is what lets a pool of workers share the
/// job: a slice needs the anchor, never the other slices.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub best: Vec<f64>,
    pub error: Vec<f64>,
    pub poi: f64,
}

impl Anchor {
    pub fn from_fit(res: &FitResult) -> Self {
        Anchor {
            best: res.params.iter().map(|p| p.value).collect(),
            error: res.params.iter().map(|p| p.error).collect(),
            poi: res.poi.value,
        }
    }
}

/// Parameters worth an impact: everything still floating except the POI.
/// Combine's `all_free_parameters()` likewise keeps unconstrained ones.
pub fn nuisances(model: &CompiledModel) -> Vec<usize> {
    (0..model.params.len())
        .filter(|&i| i != model.poi && !model.params[i].frozen)
        .collect()
}

fn kind_label(kind: &ParamKind) -> &'static str {
    match kind {
        ParamKind::Gaussian => "Gaussian",
        ParamKind::GaussianAt { sigma_lo, sigma_hi, .. } => {
            if (sigma_lo - sigma_hi).abs() < 1e-12 {
                "Gaussian"
            } else {
                "AsymmetricGaussian"
            }
        }
        ParamKind::Poisson { .. } => "Poisson",
        ParamKind::Uniform | ParamKind::Free => "Unconstrained",
        ParamKind::Poi => "POI",
    }
}

/// Impacts for a subset of nuisances, given by index into `model.params`.
pub fn impacts_for(
    model: &CompiledModel,
    cfg: &FitConfig,
    anchor: &Anchor,
    which: &[usize],
    asymmetric: bool,
) -> Vec<Impact> {
    let which: Vec<usize> = which
        .iter()
        .copied()
        .filter(|&j| j < model.params.len() && j != model.poi && !model.params[j].frozen)
        .collect();
    if which.is_empty() {
        return Vec::new();
    }
    let sigmas = nuisance_sigmas(model, cfg, anchor, &which, asymmetric);
    which.iter().zip(&sigmas).map(|(&j, &s)| one(model, cfg, anchor, j, s)).collect()
}

/// Post-fit half-widths per nuisance, and whether they are trustworthy.
///
/// The asymmetric path is Combine's default: a MINOS crossing search per
/// nuisance. All of them share one minimisation, so the cost that scales with
/// the slice is the search itself.
fn nuisance_sigmas(
    model: &CompiledModel,
    cfg: &FitConfig,
    anchor: &Anchor,
    which: &[usize],
    asymmetric: bool,
) -> Vec<(f64, f64, bool)> {
    let hesse = |j: usize| anchor.error[j].abs().max(1e-9);
    if !asymmetric {
        return which.iter().map(|&j| (hesse(j), hesse(j), true)).collect();
    }

    let nll = Nll::new(warm_from(model, anchor));
    // Starting at the minimum this costs a handful of calls; it exists only to
    // hand MINOS the FunctionMinimum and error matrix it searches out from.
    let min = crate::fit::build(&nll.model, cfg, cfg.tolerance, cfg.strategy).minimize(&nll);
    let min = MnHesse::new().with_strategy(cfg.strategy).calculate(&nll, &min);
    if !min.is_valid() {
        return which.iter().map(|&j| (hesse(j), hesse(j), false)).collect();
    }

    let m =
        MnMinos::new(&nll, &min).with_strategy(cfg.strategy).with_tolerance(cfg.minos_tolerance);
    which
        .iter()
        .map(|&j| {
            let e = m.minos(j);
            // Combine falls back to the parabolic error whenever the crossing
            // search leaves no err68 range; so do we, and say so.
            if e.is_valid() {
                (e.lower_error().abs().max(1e-9), e.upper_error().abs().max(1e-9), true)
            } else {
                (hesse(j), hesse(j), false)
            }
        })
        .collect()
}

fn warm_from(model: &CompiledModel, anchor: &Anchor) -> CompiledModel {
    let mut m = model.clone();
    for (p, v) in m.params.iter_mut().zip(&anchor.best) {
        p.init = *v;
    }
    m
}

/// The two profiled re-fits for one nuisance.
fn one(
    model: &CompiledModel,
    cfg: &FitConfig,
    anchor: &Anchor,
    j: usize,
    (sigma_lo, sigma_hi, sigma_ok): (f64, f64, bool),
) -> Impact {
    let p = &model.params[j];
    let centre = anchor.best[j];
    // A displaced value outside the parameter's own range would be rejected by
    // the minimiser, so report what we actually evaluated.
    let clamp = |v: f64| match p.bounds {
        Some((lo, hi)) => v.clamp(lo, hi),
        None => v,
    };
    let (down, up) = (clamp(centre - sigma_lo), clamp(centre + sigma_hi));

    let at = |v: f64| {
        let run = |tol: f64| {
            let mut m = warm_from(model, anchor);
            m.params[j].init = v;
            m.params[j].frozen = true;
            let cfg = FitConfig { minos: false, hesse: false, tolerance: tol, ..cfg.clone() };
            crate::fit::fit(m, &cfg)
        };
        // A nuisance pinned near its own bound can leave Migrad short of a 1e-6
        // EDM on a perfectly good minimum. Combine's default tolerance is 0.1,
        // so one retry two decades looser is still far stricter than the
        // reference, and beats reporting no shift at all.
        let r = run(cfg.tolerance).map(|f| (f, true));
        let r = r.or_else(|_| run(cfg.tolerance * 100.0).map(|f| (f, false)));
        if std::env::var_os("KESTREL_IMPACTS_DEBUG").is_some() {
            match &r {
                Ok((f, tight)) => eprintln!(
                    "impact {} at {v:.6}: r = {:.6} (edm {:.2e}, step {}, tight {tight})",
                    p.name, f.poi.value, f.edm, f.cascade_step
                ),
                Err(e) => eprintln!("impact {} at {v:.6}: FAILED: {e}", p.name),
            }
        }
        r.ok().map(|(f, tight)| (f.poi.value, tight))
    };
    let (r_down, r_up) = (at(down), at(up));

    // A nuisance that will not converge must not take the run down with it: it
    // keeps its row, with no shift and `ok` clear.
    let r_lo = r_down.map_or(anchor.poi, |(v, _)| v);
    let r_hi = r_up.map_or(anchor.poi, |(v, _)| v);
    let (w_lo, w_hi) = prior_band(&p.kind).unwrap_or((0.0, 0.0));

    Impact {
        name: p.name.clone(),
        kind: kind_label(&p.kind).to_string(),
        prefit: [p.init - w_lo, p.init, p.init + w_hi],
        fit: [down, centre, up],
        poi: [r_lo, anchor.poi, r_hi],
        impact: (r_hi - anchor.poi).abs().max((r_lo - anchor.poi).abs()),
        // Clear if anything was approximated: a fallback sigma, a loosened
        // tolerance, or a fit that never converged at all.
        ok: sigma_ok && [r_down, r_up].iter().all(|r| matches!(r, Some((_, true)))),
    }
}

/// Every nuisance, sorted the way Combine's plot does: largest impact first.
pub fn impacts(model: &CompiledModel, cfg: &FitConfig, asymmetric: bool) -> crate::Result<Vec<Impact>> {
    let res = crate::fit::fit(model.clone(), &FitConfig { minos: false, ..cfg.clone() })?;
    let anchor = Anchor::from_fit(&res);
    let mut out = impacts_for(model, cfg, &anchor, &nuisances(model), asymmetric);
    out.sort_by(|a, b| b.impact.total_cmp(&a.impact));
    Ok(out)
}
