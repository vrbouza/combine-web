//! The profile scan and the post-fit report.

use std::path::PathBuf;

use kestrel::{CompiledModel, Datacard, FitConfig, ShapeFiles, ShapeHandling};

fn model() -> CompiledModel {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let src = std::fs::read_to_string(dir.join("simple-shapes-TH1.txt")).unwrap();
    let dc = Datacard::parse(&src).unwrap();
    let names = kestrel::shapes::referenced_files(&dc, "120");
    let mut files = ShapeFiles::from_dir(&dir, &names).unwrap();
    let t = kestrel::shapes::load_with(&dc, &mut files, "120", false).unwrap();
    CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Morph).unwrap()
}

fn cfg() -> FitConfig {
    FitConfig { poi_range: (-5.0, 5.0), ..Default::default() }
}

/// Combine on this card, at tolerance 1e-6:
///   r : +0.176571  -0.496921/+0.550126
#[test]
fn the_scan_reproduces_the_minos_interval() {
    let m = model();
    let res = kestrel::fit::fit(m.clone(), &cfg()).unwrap();
    let (lo, hi) = res.minos.unwrap();
    assert!((res.poi.value - 0.176571).abs() < 1e-4, "r = {}", res.poi.value);

    let s = kestrel::scan(&m, &cfg(), (-1.0, 1.4), 120).unwrap();
    let (a, b) = s.crossings_68.expect("the scan brackets the 68% crossings");
    // The scan interpolates linearly between points, so it will not match MINOS
    // to machine precision; it must agree well enough to be the fallback.
    assert!((a - s.best_fit - lo).abs() < 5e-3, "scan {} vs minos {lo}", a - s.best_fit);
    assert!((b - s.best_fit - hi).abs() < 5e-3, "scan {} vs minos {hi}", b - s.best_fit);
}

#[test]
fn the_profile_is_a_minimum_at_the_best_fit() {
    let m = model();
    let s = kestrel::scan(&m, &cfg(), (-1.0, 1.4), 40).unwrap();
    // 2*dNLL is zero at the minimum and non-negative everywhere: this is what
    // catches the per-fit offset being differenced by mistake.
    let lowest = s.points.iter().filter(|p| p.delta_nll.is_finite())
        .map(|p| p.delta_nll).fold(f64::INFINITY, f64::min);
    assert!(lowest >= -1e-6, "profile dips below zero: {lowest}");
    assert!(lowest < 0.05, "profile never reaches the minimum: {lowest}");
    assert!(s.points.iter().all(|p| p.valid), "every point should converge");
}

/// The sharpest available check on the freeze-and-profile path: a nuisance the
/// data cannot constrain has nothing but its prior, so profiling it must
/// reproduce that prior exactly — `2*dNLL = theta^2`, crossing 1.0 at +-1.
///
/// `lumi` is kappa 1.10 on a signal of 10 against a background of 100, so the
/// data says essentially nothing about it.
#[test]
fn profiling_an_unconstrained_nuisance_reproduces_its_prior() {
    let m = model();
    let j = m.params.iter().position(|p| p.name == "lumi").unwrap();
    let s = kestrel::scan_param(&m, &cfg(), j, (-3.0, 3.0), 60).unwrap();

    assert!(s.best_fit.abs() < 1e-3, "lumi should sit at its prior centre: {}", s.best_fit);
    let (lo, hi) = s.crossings_68.expect("the prior brackets its own 68% points");
    assert!((lo + 1.0).abs() < 5e-3, "lower crossing {lo}, expected -1");
    assert!((hi - 1.0).abs() < 5e-3, "upper crossing {hi}, expected +1");

    // And the whole curve, not just the crossings.
    for p in s.points.iter().filter(|p| p.delta_nll.is_finite()) {
        assert!((p.delta_nll - p.r * p.r).abs() < 2e-2, "at {}: {} vs {}", p.r, p.delta_nll, p.r * p.r);
    }
}

/// A nuisance the data does constrain must come out narrower than its prior, by
/// the same factor `report` calls the constraint.
#[test]
fn profiling_a_constrained_nuisance_matches_its_hesse_error() {
    let m = model();
    let res = kestrel::fit::fit(m.clone(), &cfg()).unwrap();
    let rep = kestrel::report(&m, &res);
    let by = rep.pulls.iter().find(|p| p.name == "bgnorm").unwrap();

    let j = m.params.iter().position(|p| p.name == "bgnorm").unwrap();
    let s = kestrel::scan_param(&m, &cfg(), j, (-3.0, 3.0), 80).unwrap();
    let (lo, hi) = s.crossings_68.expect("the profile brackets its 68% points");

    assert!(by.constraint < 0.6, "bgnorm should be constrained: {}", by.constraint);
    // Crossings are relative to where it sat, and the half-width is the error.
    let half = 0.5 * ((hi - s.best_fit) + (s.best_fit - lo));
    assert!((half - by.error).abs() < 0.02, "profile half-width {half} vs HESSE {}", by.error);
}

/// The POI path is validated against Combine, so generalising must not move it.
#[test]
fn scanning_the_poi_by_index_is_the_old_scan() {
    let m = model();
    let a = kestrel::scan(&m, &cfg(), (-1.0, 1.4), 30).unwrap();
    let b = kestrel::scan_param(&m, &cfg(), m.poi, (-1.0, 1.4), 30).unwrap();
    assert_eq!(a.best_fit, b.best_fit);
    for (x, y) in a.points.iter().zip(&b.points) {
        assert_eq!(x.r, y.r);
        assert_eq!(x.delta_nll, y.delta_nll);
    }
}

#[test]
fn pulls_and_correlations_are_reported() {
    let m = model();
    let res = kestrel::fit::fit(m.clone(), &cfg()).unwrap();
    let rep = kestrel::report(&m, &res);

    // One entry per nuisance, the POI excluded, sorted by |pull|.
    assert_eq!(rep.pulls.len(), m.params.len() - 1);
    assert!(rep.pulls.windows(2).all(|w| w[0].pull.abs() >= w[1].pull.abs()));

    // `bgnorm` is a 30% normalisation on the dominant background, so the data
    // constrains it well below its prior. `lumi` is kappa 1.10 on a signal of
    // 10 events against a background of 100, so it barely moves and the data
    // cannot constrain it: its post-fit width stays at the prior.
    let by = |n: &str| rep.pulls.iter().find(|p| p.name == n).unwrap().clone();
    assert!(by("bgnorm").constraint < 0.6, "bgnorm: {}", by("bgnorm").constraint);
    assert!(by("lumi").pull.abs() < 1e-3, "lumi pull: {}", by("lumi").pull);
    assert!((by("lumi").constraint - 1.0).abs() < 0.02, "lumi: {}", by("lumi").constraint);

    let n = rep.names.len();
    assert_eq!(rep.correlation.len(), n * n);
    for i in 0..n {
        assert!((rep.correlation[i * n + i] - 1.0).abs() < 1e-9, "diagonal must be 1");
    }
}
