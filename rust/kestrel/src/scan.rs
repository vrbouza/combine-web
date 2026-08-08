//! Profile-likelihood scan of the POI: Combine's `MultiDimFit --algo grid`.

use serde::Serialize;

use crate::error::Result;
use crate::fit::FitConfig;
use crate::model::CompiledModel;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPoint {
    pub r: f64,
    /// `2 * (NLL(r) - NLL(r_hat))`, so 68% and 95% are at 1.0 and 3.84.
    pub delta_nll: f64,
    pub nll: f64,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub best_fit: f64,
    pub best_nll: f64,
    pub points: Vec<ScanPoint>,
    /// Where `delta_nll` crosses 1.0, if the scan brackets it.
    pub crossings_68: Option<(f64, f64)>,
    pub crossings_95: Option<(f64, f64)>,
}

/// Scan `r` over `points` values in `range`, profiling every other parameter at
/// each. Combine centres its grid, so point `i` sits at the middle of the `i`th
/// slice rather than on the endpoints.
pub fn scan(
    model: &CompiledModel,
    cfg: &FitConfig,
    range: (f64, f64),
    points: usize,
) -> Result<ScanResult> {
    scan_param(model, cfg, model.poi, range, points)
}

/// Scan one parameter, profiling every other at each point. `param` indexes
/// `model.params`; the POI takes the same path as any nuisance, and
/// `best_fit`/`ScanPoint::r` then refer to whichever parameter was scanned.
pub fn scan_param(
    model: &CompiledModel,
    cfg: &FitConfig,
    param: usize,
    range: (f64, f64),
    points: usize,
) -> Result<ScanResult> {
    if param >= model.params.len() {
        return Err(crate::Error::Inconsistent(format!(
            "no parameter with index {param}; the model has {}",
            model.params.len()
        )));
    }
    // A scan reads only the profiled likelihood and the parameter values it warm
    // starts the next point from, so the error matrix is pure cost here.
    let free =
        crate::fit::fit(model.clone(), &FitConfig { minos: false, hesse: false, ..cfg.clone() })?;
    let best_nll = free.raw_nll;
    let best_fit = free.params[param].value;

    // Starting every point from the pre-fit values leaves the harder ones in a
    // bad basin — on `templ_datacard_largeYields.txt` two points either failed
    // or converged 168 units away. Walking outwards from the minimum and warm
    // starting from the neighbour fixes that, and is much faster besides.
    let start: Vec<f64> = free.params.iter().map(|p| p.value).collect();

    let (lo, hi) = range;
    let width = (hi - lo) / points as f64;
    let grid: Vec<f64> = (0..points).map(|i| lo + width * (i as f64 + 0.5)).collect();
    let split = grid.partition_point(|&r| r < best_fit);

    let mut out: Vec<Option<ScanPoint>> = vec![None; points];
    for (indices, _) in [
        ((0..split).rev().collect::<Vec<_>>(), ()),
        ((split..points).collect::<Vec<_>>(), ()),
    ] {
        let mut warm = start.clone();
        for i in indices {
            let r = grid[i];
            let mut m = model.clone();
            for (p, v) in m.params.iter_mut().zip(&warm) {
                p.init = *v;
            }
            let one = if param == model.poi {
                crate::fit::fit(
                    m,
                    &FitConfig { poi_fixed_at: Some(r), minos: false, hesse: false, ..cfg.clone() },
                )
            } else {
                // The same freeze the impact fits use, and it leaves the POI
                // free to be profiled along with everything else.
                m.params[param].init = r;
                m.params[param].frozen = true;
                crate::fit::fit(m, &FitConfig { minos: false, hesse: false, ..cfg.clone() })
            };
            out[i] = Some(match one {
                Ok(f) => {
                    warm = f.params.iter().map(|p| p.value).collect();
                    ScanPoint {
                        r,
                        delta_nll: 2.0 * (f.raw_nll - best_nll),
                        nll: f.raw_nll,
                        valid: f.valid,
                    }
                }
                // A point that will not converge must not abort the scan; the
                // interface shows it as a gap, and the warm start is left alone
                // so the next point still has a sane origin.
                Err(_) => ScanPoint { r, delta_nll: f64::NAN, nll: f64::NAN, valid: false },
            });
        }
    }

    let out: Vec<ScanPoint> = out.into_iter().map(|p| p.expect("every grid point is visited")).collect();
    let crossings_68 = crossings(&out, best_fit, 1.0);
    let crossings_95 = crossings(&out, best_fit, 3.84);
    Ok(ScanResult { best_fit, best_nll, points: out, crossings_68, crossings_95 })
}

/// Linear interpolation of where `delta_nll` reaches `level`, either side of the
/// best fit.
fn crossings(points: &[ScanPoint], best: f64, level: f64) -> Option<(f64, f64)> {
    let good: Vec<&ScanPoint> = points.iter().filter(|p| p.delta_nll.is_finite()).collect();
    let cross = |a: &ScanPoint, b: &ScanPoint| {
        a.r + (b.r - a.r) * (level - a.delta_nll) / (b.delta_nll - a.delta_nll)
    };
    let mut down = None;
    let mut up = None;
    for w in good.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (a.delta_nll - level) * (b.delta_nll - level) > 0.0 {
            continue;
        }
        if b.r <= best {
            down = Some(cross(a, b));
        } else if a.r >= best {
            if up.is_none() {
                up = Some(cross(a, b));
            }
        } else {
            // The bracket straddles the minimum; assign by which side is lower.
            if a.delta_nll > b.delta_nll { down = Some(cross(a, b)) } else { up = Some(cross(a, b)) }
        }
    }
    Some((down?, up?))
}
