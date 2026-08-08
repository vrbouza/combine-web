//! The flat parameter vector the minimiser sees, and the constraint terms.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ParamKind {
    /// The signal strength. Unconstrained, but range-limited.
    Poi,
    /// Unit Gaussian at zero: `lnN`, `shape`, and Barlow-Beeston parameters.
    Gaussian,
    /// Flat prior over the bounds: `lnU`, `shapeU`.
    Uniform,
    /// `gmN`: the sideband yield, Poisson-constrained on `n_obs` observed events.
    Poisson { n_obs: f64 },
    /// `param`: a Gaussian of arbitrary centre and width.
    GaussianAt { mean: f64, sigma_lo: f64, sigma_hi: f64 },
    /// `flatParam`, `rateParam`: floats freely, no constraint term.
    Free,
}

#[derive(Debug, Clone, Serialize)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
    pub init: f64,
    pub bounds: Option<(f64, f64)>,
    /// Set by `[nofloat]`, or by the caller freezing it.
    pub frozen: bool,
}

impl Param {
    /// Contribution of this parameter's prior to the negative log-likelihood.
    /// Constants are dropped: only the shape in the parameter matters.
    pub fn constraint(&self, v: f64) -> f64 {
        match self.kind {
            ParamKind::Gaussian => 0.5 * v * v,
            ParamKind::GaussianAt { mean, sigma_lo, sigma_hi } => {
                let d = v - mean;
                let s = if d < 0.0 { sigma_lo } else { sigma_hi };
                if s > 0.0 {
                    0.5 * (d / s).powi(2)
                } else {
                    0.0
                }
            }
            // nu is the Poisson mean and n_obs the sideband count.
            ParamKind::Poisson { n_obs } => {
                let nu = v.max(1e-12);
                nu - n_obs * nu.ln()
            }
            ParamKind::Poi | ParamKind::Uniform | ParamKind::Free => 0.0,
        }
    }

    /// d(constraint)/dv.
    pub fn constraint_grad(&self, v: f64) -> f64 {
        match self.kind {
            ParamKind::Gaussian => v,
            ParamKind::GaussianAt { mean, sigma_lo, sigma_hi } => {
                let d = v - mean;
                let s = if d < 0.0 { sigma_lo } else { sigma_hi };
                if s > 0.0 {
                    d / (s * s)
                } else {
                    0.0
                }
            }
            ParamKind::Poisson { n_obs } => 1.0 - n_obs / v.max(1e-12),
            ParamKind::Poi | ParamKind::Uniform | ParamKind::Free => 0.0,
        }
    }
}
