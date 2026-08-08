//! Post-fit diagnostics: nuisance pulls and the correlation matrix.

use serde::Serialize;

use crate::fit::FitResult;
use crate::model::{CompiledModel, ParamKind};

/// One nuisance, pre-fit against post-fit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub name: String,
    /// Post-fit value in units of the pre-fit width, so a unit-Gaussian
    /// nuisance reads directly as "how far it moved, in sigma".
    pub pull: f64,
    /// Post-fit uncertainty over the pre-fit one. Below 1 means the data
    /// constrained this nuisance beyond its prior.
    pub constraint: f64,
    pub value: f64,
    pub error: f64,
    /// Free parameters have no prior, so `pull` is not meaningful for them.
    pub constrained: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub pulls: Vec<Pull>,
    /// Row-major correlation matrix over `names`, or empty if HESSE gave no
    /// usable covariance.
    pub names: Vec<String>,
    pub correlation: Vec<f64>,
}

/// Pre-fit half-widths either side of the prior centre. `None` for parameters
/// with no prior, which the impacts plot draws differently.
pub fn prior_band(kind: &ParamKind) -> Option<(f64, f64)> {
    match kind {
        ParamKind::Gaussian => Some((1.0, 1.0)),
        ParamKind::GaussianAt { sigma_lo, sigma_hi, .. } => Some((*sigma_lo, *sigma_hi)),
        // A Poisson sideband of n observed events has width sqrt(n+1).
        ParamKind::Poisson { n_obs } => {
            let w = (n_obs + 1.0).sqrt();
            Some((w, w))
        }
        ParamKind::Uniform | ParamKind::Free | ParamKind::Poi => None,
    }
}

/// The pre-fit width of a parameter's prior, used to normalise its pull.
fn prior_width(kind: &ParamKind) -> Option<f64> {
    prior_band(kind).map(|(lo, hi)| 0.5 * (lo + hi))
}

pub fn report(model: &CompiledModel, res: &FitResult) -> Report {
    let mut pulls = Vec::new();
    for (i, p) in model.params.iter().enumerate() {
        if i == model.poi {
            continue;
        }
        let f = &res.params[i];
        let width = prior_width(&p.kind);
        pulls.push(Pull {
            name: p.name.clone(),
            pull: width.map_or(0.0, |w| (f.value - p.init) / w),
            constraint: width.map_or(1.0, |w| f.error / w),
            value: f.value,
            error: f.error,
            constrained: width.is_some(),
        });
    }
    // Largest movers first: that is the order a reader wants.
    pulls.sort_by(|a, b| b.pull.abs().total_cmp(&a.pull.abs()));

    let names: Vec<String> = res.params.iter().map(|p| p.name.clone()).collect();
    Report { pulls, names, correlation: res.correlation.clone() }
}
