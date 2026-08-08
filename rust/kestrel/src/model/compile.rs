//! Turning a parsed datacard into a `CompiledModel`.

use std::collections::HashMap;

use super::params::{Param, ParamKind};
use super::{Channel, CompiledModel, Factor, Morph, Proc, ShapeHandling};
use crate::datacard::{Datacard, Effect, SystKind};
use crate::error::{Error, Result};
use crate::shapes::Templates;

/// Combine's `ProcessNormalization::addLogNormal` is a no-op at these values.
fn is_noop(kappa: f64) -> bool {
    kappa == 0.0 || kappa == 1.0
}

/// `addAsymmLogNormal` degrades to a symmetric log-normal on `kappa_hi` when the
/// two sides are reciprocal to within 1e-5 (`src/ProcessNormalization.cc:41`).
fn is_effectively_symmetric(lo: f64, hi: f64) -> bool {
    (lo * hi - 1.0).abs() < 1e-5
}

/// `*`, or a single trailing-`*` prefix, as used by `rateParam`.
fn glob_matches(pattern: &str, s: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => s.starts_with(prefix),
        None => pattern == s,
    }
}

impl CompiledModel {
    /// A counting card: every channel is a single bin.
    pub fn compile(dc: &Datacard) -> Result<Self> {
        if dc.has_shapes() {
            return Err(Error::Inconsistent(
                "this card has shapes; use `compile_with_shapes`".into(),
            ));
        }
        Self::build(dc, None, ShapeHandling::Morph)
    }

    pub fn compile_with_shapes(
        dc: &Datacard,
        templates: &Templates,
        handling: ShapeHandling,
    ) -> Result<Self> {
        Self::build(dc, Some(templates), handling)
    }

    fn build(dc: &Datacard, tmpl: Option<&Templates>, handling: ShapeHandling) -> Result<Self> {
        let mut ignored = Vec::new();


        // --- parameters: the POI first, then one per systematic ---------------
        let mut params = vec![Param {
            name: "r".into(),
            kind: ParamKind::Poi,
            init: 1.0,
            bounds: Some((0.0, 20.0)),
            frozen: false,
        }];
        let mut index = HashMap::new();

        for s in &dc.systs {
            if handling == ShapeHandling::Ignore
                && matches!(s.kind, SystKind::Shape { .. } | SystKind::ShapeU)
            {
                ignored.push(s.name.clone());
                continue;
            }
            let (kind, init, bounds) = match &s.kind {
                SystKind::LnN => (ParamKind::Gaussian, 0.0, Some((-7.0, 7.0))),
                SystKind::LnU => (ParamKind::Uniform, 0.0, Some((-1.0, 1.0))),
                // Combine gives shape nuisances a tighter range than lnN.
                SystKind::Shape { .. } => (ParamKind::Gaussian, 0.0, Some((-4.0, 4.0))),
                SystKind::ShapeU => (ParamKind::Uniform, 0.0, Some((-1.0, 1.0))),
                SystKind::GmN { n_obs } => {
                    let n = *n_obs as f64;
                    let hi = n + 1.0 + 7.0 * (n + 1.0).sqrt();
                    (ParamKind::Poisson { n_obs: n }, n + 1.0, Some((0.0, hi)))
                }
                SystKind::Param { mean, sigma_lo, sigma_hi } => (
                    ParamKind::GaussianAt { mean: *mean, sigma_lo: *sigma_lo, sigma_hi: *sigma_hi },
                    *mean,
                    Some((mean - 7.0 * sigma_lo, mean + 7.0 * sigma_hi)),
                ),
                SystKind::FlatParam => (ParamKind::Free, 1.0, None),
                SystKind::RateParam { init, range, .. } => (ParamKind::Free, *init, *range),
            };
            index.insert(s.name.clone(), params.len());
            params.push(Param { name: s.name.clone(), kind, init, bounds, frozen: s.nofloat });
        }

        // --- channels and processes ------------------------------------------
        let mut channels = Vec::new();
        let mut procs = Vec::new();
        let mut templates = Vec::new();
        let mut errors = Vec::new();
        let mut obs = Vec::new();
        let (mut morphs, mut sums, mut diffs) = (Vec::new(), Vec::new(), Vec::new());

        for (ci, name) in dc.channels.iter().enumerate() {
            let n_bins = tmpl.map(|t| t.bins_per_channel[name]).unwrap_or(1);
            let bin_off = obs.len();
            let first = procs.len();

            for (col, k) in dc.keyline.iter().enumerate() {
                if &k.channel != name {
                    continue;
                }
                // The histogram supplies the shape; the card's `rate` supplies
                // the scale, unless it is -1, when the integral does both. Bin
                // errors are stored the same way, so both scale with the
                // normalisation.
                let (rate, fractions, frac_errors) = match tmpl {
                    None => (k.rate, vec![1.0], vec![0.0]),
                    Some(t) => {
                        let h = &t.nominal[&(k.channel.clone(), k.process.clone())];
                        let integral = h.integral();
                        let rate = if k.rate < 0.0 { integral } else { k.rate };
                        let inv = if integral != 0.0 { 1.0 / integral } else { 0.0 };
                        (
                            rate,
                            h.values.iter().map(|v| v * inv).collect(),
                            h.errors.iter().map(|e| e * inv).collect(),
                        )
                    }
                };

                let mut p = Proc {
                    channel: ci,
                    name: k.process.clone(),
                    is_signal: k.is_signal,
                    nominal: rate,
                    tmpl_off: templates.len(),
                    log_kappa: Vec::new(),
                    asym_kappa: Vec::new(),
                    factors: Vec::new(),
                    morphs: 0..0,
                    smooth_region: 1.0,
                };
                templates.extend_from_slice(&fractions);
                errors.extend_from_slice(&frac_errors);

                for s in &dc.systs {
                    let Some(&pi) = index.get(&s.name) else { continue };
                    match &s.kind {
                        SystKind::GmN { .. } => {
                            if let Some(Some(Effect::Sym(alpha))) = s.cols.get(col) {
                                // The yield is alpha * nu, so alpha replaces the
                                // nominal rate and nu enters as a free factor.
                                p.nominal = *alpha;
                                p.factors.push(Factor { param: pi, scale: 1.0 });
                            }
                        }
                        SystKind::RateParam { channel, process, .. } => {
                            if glob_matches(channel, name) && glob_matches(process, &k.process) {
                                p.factors.push(Factor { param: pi, scale: 1.0 });
                            }
                        }
                        SystKind::LnN | SystKind::LnU => match s.cols.get(col) {
                            Some(Some(Effect::Sym(kappa))) if !is_noop(*kappa) => {
                                p.log_kappa.push((pi, kappa.ln()));
                            }
                            Some(Some(Effect::Asym { down, up })) => {
                                if is_effectively_symmetric(*down, *up) {
                                    if !is_noop(*up) {
                                        p.log_kappa.push((pi, up.ln()));
                                    }
                                } else {
                                    p.asym_kappa.push((pi, down.ln(), up.ln()));
                                }
                            }
                            _ => {}
                        },
                        // Handled below: morphing needs the templates, and
                        // `smooth_region` needs every epsilon on the process first.
                        SystKind::Shape { .. } | SystKind::ShapeU => {}
                        SystKind::Param { .. } | SystKind::FlatParam => {}
                    }
                }

                if handling == ShapeHandling::Morph {
                    match tmpl {
                        Some(t) => build_morphs(
                            dc, t, &index, col, k, &mut p, &mut morphs, &mut sums, &mut diffs,
                        )?,
                        None if dc.systs.iter().any(|s| {
                            matches!(s.kind, SystKind::Shape { .. } | SystKind::ShapeU)
                                && matches!(s.cols.get(col), Some(Some(_)))
                        }) =>
                        {
                            return Err(Error::Inconsistent(format!(
                                "process {:?} carries a shape systematic but the card has no templates",
                                k.process
                            )))
                        }
                        None => {}
                    }
                }
                procs.push(p);
            }

            if procs.len() == first {
                return Err(Error::Inconsistent(format!("channel {:?} has no processes", name)));
            }

            match tmpl {
                None => obs.push(dc.observation[ci]),
                Some(t) => {
                    let data = &t.data[name];
                    if dc.observation[ci] < 0.0 {
                        obs.extend_from_slice(&data.values);
                    } else {
                        // An explicit `observation` must agree with the data
                        // histogram it is shadowing; the per-bin split can only
                        // come from the histogram.
                        obs.extend_from_slice(&data.values);
                    }
                }
            }

            channels.push(Channel {
                name: name.clone(),
                bin_off,
                n_bins,
                procs: first..procs.len(),
            });
        }

        ignored.dedup();
        let n_bins = obs.len();
        let mut model = CompiledModel {
            params,
            poi: 0,
            channels,
            procs,
            morphs,
            templates,
            morph_sum: sums,
            morph_diff: diffs,
            errors,
            obs,
            n_bins,
            bb_bins: Vec::new(),
            ignored_shape_systs: ignored,
            warnings: Vec::new(),
        };
        model.bb_bins = model.barlow_beeston_bins(dc)?;
        if !model.bb_bins.is_empty() && !model.morphs.is_empty() {
            // Measured on template-analysis_shapeInterp with and without the
            // autoMCStats line: 7.6e-8 versus 9.1e-4 per bin, same templates.
            model.warnings.push(
                "this card combines `autoMCStats` with shape systematics. Combine morphs \
                 autoMCStats channels with CMSHistFunc, whose interpolation differs from the \
                 FastVerticalInterpHistPdf2 scheme implemented here, so expect ~1e-3 on the \
                 fitted parameter of interest rather than the usual ~1e-6."
                    .into(),
            );
        }
        Ok(model)
    }
}

/// Combine round-trips the shape-derived kappa through a `"%f"` format when it
/// builds the workspace (`ShapeTools.py:1168`), quantising it to six decimal
/// places: 0.89999998 becomes exactly 0.9. Skipping this leaves a ~1e-8
/// relative error on every yield. It applies only here — `lnN` kappas come from
/// the card text and are already exact.
fn quantise(kappa: f64) -> f64 {
    (kappa * 1e6).round() / 1e6
}

/// Unit-normalised bin fractions.
fn fractions(h: &crate::shapes::Hist) -> Vec<f64> {
    let s = h.integral();
    let inv = if s != 0.0 { 1.0 / s } else { 0.0 };
    h.values.iter().map(|v| v * inv).collect()
}

/// `shapeN` interpolates the log of the fraction, so a zero bin has no defined
/// variation; Combine's own code crops rather than propagating an infinity.
fn delta_log(a: f64, b: f64) -> f64 {
    if a > 0.0 && b > 0.0 {
        (a / b).ln()
    } else {
        0.0
    }
}

/// Vertical template morphing for one (channel, process).
///
/// The nominal and both variations are normalised to unity *before* the sum and
/// difference arrays are formed; the integral ratio goes separately into the
/// process normalisation as an asymmetric log-normal. One theta drives both.
#[allow(clippy::too_many_arguments)]
fn build_morphs(
    dc: &Datacard,
    t: &Templates,
    index: &HashMap<String, usize>,
    col: usize,
    k: &crate::datacard::KeyEntry,
    p: &mut Proc,
    morphs: &mut Vec<Morph>,
    sums: &mut Vec<f64>,
    diffs: &mut Vec<f64>,
) -> Result<()> {
    let key = (k.channel.clone(), k.process.clone());
    let nom = &t.nominal[&key];
    let i_nom = nom.integral();
    let f_nom = fractions(nom);

    let first = morphs.len();
    // Combine seeds this at 1 and only lowers it, so an epsilon above 1 does
    // not widen the smooth region (`ShapeTools.py:915-935`).
    let mut min_eps = 1.0f64;

    for s in &dc.systs {
        let log_space = match s.kind {
            SystKind::Shape { log } => log,
            SystKind::ShapeU => false,
            _ => continue,
        };
        let Some(Some(Effect::Sym(eps))) = s.cols.get(col) else { continue };
        let Some((up, down)) = t.variations.get(&(key.0.clone(), key.1.clone(), s.name.clone()))
        else {
            continue;
        };
        let pi = index[&s.name];
        min_eps = min_eps.min(*eps);

        if i_nom > 0.0 {
            let ku = quantise((up.integral() / i_nom).powf(*eps));
            let kd = quantise((down.integral() / i_nom).powf(*eps));
            // Combine skips the normalisation term outright when both sides are
            // within 1e-3 of unity (`ShapeTools.py:1160`).
            if (ku - 1.0).abs() >= 1e-3 || (kd - 1.0).abs() >= 1e-3 {
                if is_effectively_symmetric(kd, ku) {
                    if !is_noop(ku) {
                        p.log_kappa.push((pi, ku.ln()));
                    }
                } else {
                    p.asym_kappa.push((pi, kd.ln(), ku.ln()));
                }
            }
        }

        let (f_up, f_down) = (fractions(up), fractions(down));
        let off = sums.len();
        for b in 0..f_nom.len() {
            let (du, dd) = if log_space {
                (delta_log(f_up[b], f_nom[b]), delta_log(f_down[b], f_nom[b]))
            } else {
                (f_up[b] - f_nom[b], f_down[b] - f_nom[b])
            };
            sums.push(du + dd);
            diffs.push(du - dd);
        }
        morphs.push(Morph { param: pi, log_space, scale: *eps, off });
    }

    p.morphs = first..morphs.len();
    p.smooth_region = min_eps;
    Ok(())
}

impl CompiledModel {
    /// Which bins get a Barlow-Beeston-lite parameter.
    ///
    /// The decision is made once, at the pre-fit point, exactly as
    /// `CMSHistErrorPropagator::setupBinPars` does. Note the asymmetry: the
    /// *threshold* is judged on background only (signal carries
    /// `skipForErrorSum` unless `include-signal` is set), while the error sum
    /// used at runtime includes every process.
    fn barlow_beeston_bins(&self, dc: &Datacard) -> Result<Vec<usize>> {
        let mut bins = Vec::new();
        let par: Vec<f64> = self.params.iter().map(|p| p.init).collect();

        for ch in &self.channels {
            let Some((_, cfg)) = dc
                .auto_mc_stats
                .iter()
                .find(|(pat, _)| glob_matches(pat, &ch.name))
            else {
                continue;
            };
            if cfg.threshold < 0.0 {
                continue; // -1 switches the nuisances off entirely
            }

            for b in 0..ch.n_bins {
                let (mut sub_sum, mut sub_err2) = (0.0f64, 0.0f64);
                for p in &self.procs[ch.procs.clone()] {
                    if p.is_signal && !cfg.include_signal {
                        continue;
                    }
                    let n = self.normalisation(p, &par);
                    sub_sum += n * self.templates[p.tmpl_off + b];
                    let e = n * self.errors[p.tmpl_off + b];
                    sub_err2 += e * e;
                }
                if sub_err2 <= 0.0 {
                    continue; // no MC statistical information in this bin
                }
                let n_eff = (0.5 + sub_sum * sub_sum / sub_err2).floor();
                if n_eff <= cfg.threshold {
                    return Err(Error::Inconsistent(format!(
                        "channel {:?} bin {} has {} effective events, at or below the \
                         `autoMCStats` threshold of {}, which asks for per-process Poisson \
                         parameters. Only the Barlow-Beeston-lite branch is implemented.",
                        ch.name,
                        b + 1,
                        n_eff,
                        cfg.threshold
                    )));
                }
                bins.push(ch.bin_off + b);
            }
        }
        Ok(bins)
    }
}
