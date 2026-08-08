//! Nuisance impacts, against real Combine 11.0.0 and for internal consistency.

use std::path::PathBuf;

use kestrel::{Anchor, CompiledModel, Datacard, FitConfig, ShapeFiles, ShapeHandling};

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

fn all(m: &CompiledModel, asymmetric: bool) -> Vec<kestrel::Impact> {
    let res = kestrel::fit::fit(m.clone(), &FitConfig { minos: false, ..cfg() }).unwrap();
    let anchor = Anchor::from_fit(&res);
    kestrel::impacts_for(m, &cfg(), &anchor, &kestrel::impacts::nuisances(m), asymmetric)
}

/// Real Combine 11.0.0 at tolerance 1e-6, via `scripts/combine_impacts.py`.
/// Signed shifts of `r`, as `plotImpacts.py` derives them from the triplets.
#[test]
fn impacts_reproduce_combine() {
    let m = model();
    let out = all(&m, true);
    let by = |n: &str| out.iter().find(|i| i.name == n).unwrap().clone();

    for (name, lo, hi) in [
        ("alpha", 0.295658, -0.287932),
        ("bgnorm", 0.109968, -0.110913),
        ("lumi", 0.017655, -0.016053),
        ("sigma", -0.003522, 0.015153),
    ] {
        let i = by(name);
        assert!(i.ok, "{name} did not converge");
        assert!((i.poi[0] - i.poi[1] - lo).abs() < 1e-4, "{name} impact-: {}", i.poi[0] - i.poi[1]);
        assert!((i.poi[2] - i.poi[1] - hi).abs() < 1e-4, "{name} impact+: {}", i.poi[2] - i.poi[1]);
    }
}

#[test]
fn the_ranking_matches_the_physics() {
    let m = model();
    let mut out = all(&m, true);
    out.sort_by(|a, b| b.impact.total_cmp(&a.impact));

    // `alpha` morphs the dominant background's shape and `bgnorm` scales it by
    // 30%; `lumi` is 10% on a signal of 10 against a background of 100. The
    // ordering is the whole point of the plot, so it is worth asserting.
    assert_eq!(out[0].name, "alpha");
    assert_eq!(out[1].name, "bgnorm");
    assert!(out[0].impact > 2.0 * out[1].impact);
    assert!(out.windows(2).all(|w| w[0].impact >= w[1].impact));
}

#[test]
fn every_floating_nuisance_gets_a_row_and_the_poi_does_not() {
    let m = model();
    let out = all(&m, true);
    assert_eq!(out.len(), m.params.len() - 1);
    assert!(!out.iter().any(|i| i.name == "r"));
    // The POI triplet is centred on the global best fit, identically for all.
    assert!(out.windows(2).all(|w| w[0].poi[1] == w[1].poi[1]));
}

#[test]
fn a_frozen_parameter_is_skipped() {
    let mut m = model();
    let j = m.params.iter().position(|p| p.name == "lumi").unwrap();
    m.params[j].frozen = true;
    let out = all(&m, true);
    assert!(!out.iter().any(|i| i.name == "lumi"));
    assert_eq!(out.len(), m.params.len() - 2);
}

/// Slicing is what the worker pool relies on, so it must not change an answer.
#[test]
fn slicing_gives_the_same_answer_as_one_call() {
    let m = model();
    let res = kestrel::fit::fit(m.clone(), &FitConfig { minos: false, ..cfg() }).unwrap();
    let anchor = Anchor::from_fit(&res);
    let which = kestrel::impacts::nuisances(&m);

    let whole = kestrel::impacts_for(&m, &cfg(), &anchor, &which, true);
    let sliced: Vec<_> = which
        .chunks(2)
        .flat_map(|c| kestrel::impacts_for(&m, &cfg(), &anchor, c, true))
        .collect();

    assert_eq!(whole.len(), sliced.len());
    for (a, b) in whole.iter().zip(&sliced) {
        assert_eq!(a.name, b.name);
        assert!((a.poi[0] - b.poi[0]).abs() < 1e-9, "{}: {} vs {}", a.name, a.poi[0], b.poi[0]);
        assert!((a.poi[2] - b.poi[2]).abs() < 1e-9, "{}: {} vs {}", a.name, a.poi[2], b.poi[2]);
    }
}

/// The symmetric mode exists so the browser can trade fidelity for time; it
/// must land close enough to be a defensible default when a crossing fails.
#[test]
fn symmetric_errors_track_the_minos_ones() {
    let m = model();
    let asym = all(&m, true);
    let sym = all(&m, false);
    for (a, s) in asym.iter().zip(&sym) {
        assert_eq!(a.name, s.name);
        let (ai, si) = (a.impact, s.impact);
        assert!((ai - si).abs() < 0.05 * ai.max(1e-3), "{}: {ai} vs {si}", a.name);
    }
}

#[test]
fn prefit_bands_follow_the_prior() {
    let m = model();
    let out = all(&m, true);
    // Every nuisance on this card is a unit Gaussian, so the pre-fit band is
    // exactly [-1, 0, 1] — which is what Combine writes into impacts.json.
    for i in &out {
        assert_eq!(i.kind, "Gaussian", "{}", i.name);
        assert_eq!(i.prefit, [-1.0, 0.0, 1.0], "{}", i.name);
    }
}
