//! The fit driver: parameter setup, a minimiser cascade, HESSE and MINOS.

use minuit2::{MnHesse, MnMigrad, MnMinos, MnSimplex};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::{CompiledModel, ParamKind};
use crate::nll::Nll;

#[derive(Debug, Clone)]
pub struct FitConfig {
    pub poi_range: (f64, f64),
    /// Combine defaults to 0.1, which measurably under-converges: on the
    /// reference counting card it stops 5e-3 from the true minimum, and on
    /// `simple-shapes-TH1` even 1e-4 leaves 1e-4 on the table. 1e-6 costs
    /// nothing at this scale and agrees with Combine to 3e-6.
    pub tolerance: f64,
    /// MnMinos defaults to 0.1, which stops the crossing search well short: on
    /// `rate_params/signal_region.txt` that costs 6e-3 on the lower bound,
    /// against 1e-4 here.
    pub minos_tolerance: f64,
    pub strategy: u32,
    pub minos: bool,
    /// HESSE costs O(n^2) second derivatives, which on a large card dwarfs the
    /// minimisation itself. Callers that only want the profiled parameter
    /// values — impact fits, scan points — turn it off.
    pub hesse: bool,
    /// Freeze the POI at zero, i.e. Combine's background-only fit.
    pub background_only: bool,
    /// Freeze the POI at this value and profile everything else — one point of
    /// a likelihood scan. Takes precedence over `background_only`.
    pub poi_fixed_at: Option<f64>,
}

impl Default for FitConfig {
    fn default() -> Self {
        FitConfig {
            poi_range: (0.0, 20.0),
            tolerance: 1e-6,
            minos_tolerance: 1e-4,
            strategy: 1,
            minos: true,
            hesse: true,
            background_only: false,
            poi_fixed_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FittedParam {
    pub name: String,
    pub value: f64,
    pub error: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitResult {
    pub params: Vec<FittedParam>,
    pub poi: FittedParam,
    pub minos: Option<(f64, f64)>,
    /// Offset at the pre-fit point, as Combine reports it. Not comparable
    /// between fits, since each has its own offset.
    pub nll: f64,
    /// The un-offset likelihood. This is the one to difference across fits.
    pub raw_nll: f64,
    pub edm: f64,
    pub nfcn: usize,
    pub valid: bool,
    /// Which cascade step produced this result; 0 means the first try.
    pub cascade_step: u32,
    /// Row-major correlation matrix over `params`, empty if HESSE gave none.
    pub correlation: Vec<f64>,
}

/// How one parameter is presented to the minimiser.
struct Slot<'a> {
    name: &'a str,
    init: f64,
    bounds: Option<(f64, f64)>,
    fixed: bool,
}

fn slots<'a>(model: &'a CompiledModel, cfg: &FitConfig, at: Option<&[f64]>) -> Vec<Slot<'a>> {
    model
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_poi = i == model.poi;
            let bounds = if is_poi { Some(cfg.poi_range) } else { p.bounds };
            // Unit-Gaussian nuisances are effectively unbounded near zero, and
            // Migrad converges better without the sin/asin transform, so their
            // nominal +-7 range is dropped.
            let wide = matches!(p.kind, ParamKind::Gaussian) && !is_poi;
            Slot {
                name: &p.name,
                init: at.and_then(|v| v.get(i).copied()).unwrap_or(p.init),
                bounds: if wide { None } else { bounds },
                fixed: p.frozen || (is_poi && (cfg.background_only || cfg.poi_fixed_at.is_some())),
            }
        })
        .collect()
}

pub(crate) fn build(model: &CompiledModel, cfg: &FitConfig, tol: f64, strategy: u32) -> MnMigrad {
    build_at(model, cfg, tol, strategy, None)
}

fn build_at(
    model: &CompiledModel,
    cfg: &FitConfig,
    tol: f64,
    strategy: u32,
    at: Option<&[f64]>,
) -> MnMigrad {
    let mut m = MnMigrad::new();
    for (i, s) in slots(model, cfg, at).iter().enumerate() {
        m = match s.bounds {
            Some((lo, hi)) => m.add_limited(s.name, s.init, 1.0, lo, hi),
            None => m.add(s.name, s.init, 1.0),
        };
        if s.fixed {
            m = m.fix(i);
        }
    }
    m.tolerance(tol).with_strategy(strategy)
}

fn build_simplex(model: &CompiledModel, cfg: &FitConfig, tol: f64) -> MnSimplex {
    let mut m = MnSimplex::new();
    for (i, s) in slots(model, cfg, None).iter().enumerate() {
        m = match s.bounds {
            Some((lo, hi)) => m.add_limited(s.name, s.init, 1.0, lo, hi),
            None => m.add(s.name, s.init, 1.0),
        };
        if s.fixed {
            m = m.fix(i);
        }
    }
    m.tolerance(tol)
}

/// Minimise, escalating through Combine's `CascadeMinimizer` idea: a cheap pass
/// first, then a careful one, then a Simplex reseed.
pub fn fit(model: CompiledModel, cfg: &FitConfig) -> Result<FitResult> {
    let want_minos = cfg.minos && !cfg.background_only && cfg.poi_fixed_at.is_none();
    // Only the MINOS scan fallback needs a pristine copy, and cloning the
    // templates is not free when this runs thousands of times.
    let for_fallback = want_minos.then(|| model.clone());
    let mut model = model;
    if let Some(v) = cfg.poi_fixed_at {
        model.params[model.poi].init = v;
    } else if cfg.background_only {
        model.params[model.poi].init = 0.0;
    }
    let poi = model.poi;
    let nll = Nll::new(model);

    let steps: [(f64, u32); 3] =
        [(cfg.tolerance, 0), (cfg.tolerance * 0.1, cfg.strategy.max(1)), (cfg.tolerance * 0.1, 2)];

    let mut best = None;
    for (step, &(tol, strategy)) in steps.iter().enumerate() {
        let min = build(&nll.model, cfg, tol, strategy).minimize(&nll);
        if min.is_valid() {
            best = Some((step as u32, min));
            break;
        }
        best = Some((step as u32, min));
    }

    // Last resort, and the step Combine's CascadeMinimizer reaches for: a
    // derivative-free pass to walk out of the bad basin, then Migrad again from
    // where it lands. Rare, but it is what rescues a nuisance pinned near its
    // bound during an impact fit.
    if !best.as_ref().is_some_and(|(_, m)| m.is_valid()) {
        let seed = build_simplex(&nll.model, cfg, cfg.tolerance).minimize(&nll);
        let at: Vec<f64> =
            seed.user_state().params().params().iter().map(|p| p.value()).collect();
        let m = build_at(&nll.model, cfg, cfg.tolerance * 0.1, 2, Some(&at)).minimize(&nll);
        if m.is_valid() {
            best = Some((3, m));
        }
    }

    let (cascade_step, min) = best.expect("the cascade always yields a result");
    if !min.is_valid() {
        return Err(Error::Converge(format!(
            "Migrad did not converge after {} attempts (edm = {:.3e})",
            steps.len(),
            min.edm()
        )));
    }

    // MINOS starts its crossing search from the error matrix, so it needs HESSE
    // whether or not the caller asked for it.
    let min = if cfg.hesse || want_minos {
        MnHesse::new().with_strategy(cfg.strategy).calculate(&nll, &min)
    } else {
        min
    };
    let st = min.user_state();
    let params: Vec<FittedParam> = st
        .params()
        .params()
        .iter()
        .map(|p| FittedParam { name: p.name().to_string(), value: p.value(), error: p.error() })
        .collect();

    // HESSE's covariance, normalised to correlations. Parameters Migrad held
    // fixed have no row, so index by name rather than position.
    let n = params.len();
    let mut correlation = Vec::new();
    if let Some(cov) = st.covariance() {
        let var: Vec<f64> = (0..cov.nrow()).map(|i| cov.get(i, i)).collect();
        // Map each free parameter back onto its slot in the full list.
        let free: Vec<usize> = (0..n)
            .filter(|&i| !nll.model.params[i].frozen)
            .take(cov.nrow())
            .collect();
        correlation = vec![0.0; n * n];
        for (a, &ia) in free.iter().enumerate() {
            for (b, &ib) in free.iter().enumerate() {
                let d = (var[a] * var[b]).sqrt();
                correlation[ia * n + ib] = if d > 0.0 { cov.get(a, b) / d } else { 0.0 };
            }
        }
    }

    let minos = if want_minos {
        let m = MnMinos::new(&nll, &min)
            .with_strategy(cfg.strategy)
            .with_tolerance(cfg.minos_tolerance);
        let e = m.minos(poi);
        if std::env::var_os("KESTREL_MINOS_DEBUG").is_some() {
            eprintln!(
                "minos: valid={} lower_valid={} upper_valid={} at_lo_limit={} at_hi_limit={} \
                 lower={:.6} upper={:.6}",
                e.is_valid(),
                e.lower_valid(),
                e.upper_valid(),
                e.at_lower_limit(),
                e.at_upper_limit(),
                e.lower_error(),
                e.upper_error()
            );
        }
        if e.is_valid() {
            Some((e.lower_error(), e.upper_error()))
        } else {
            // MnMinos re-fits with the POI fixed, and on stiff cards those fits
            // land in bad basins and it gives up, returning the symmetric
            // parabolic error. A warm-started scan finds the same crossings and
            // does not. Combine reaches for the same remedy with `--robustFit 1`.
            let hesse = params[poi].error.max(1e-6);
            let centre = params[poi].value;
            let (lo, hi) = (centre - 3.0 * hesse, centre + 3.0 * hesse);
            let range = (lo.max(cfg.poi_range.0), hi.min(cfg.poi_range.1));
            for_fallback
                .as_ref()
                .and_then(|m| {
                    crate::scan::scan(m, &FitConfig { minos: false, ..cfg.clone() }, range, 60).ok()
                })
                .and_then(|s| s.crossings_68.map(|(a, b)| (a - s.best_fit, b - s.best_fit)))
        }
    } else {
        None
    };

    Ok(FitResult {
        poi: params[poi].clone(),
        params,
        minos,
        nll: min.fval(),
        raw_nll: min.fval() + nll.offset(),
        edm: min.edm(),
        nfcn: min.nfcn(),
        valid: min.is_valid(),
        cascade_step,
        correlation,
    })
}
