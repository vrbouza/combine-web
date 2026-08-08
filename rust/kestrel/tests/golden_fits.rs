//! Fits with published reference values.
//!
//! Combine's own numbers come from `docs/tutorial_stat_routines/stat_routines.md`.
//! Note they are converged only to Combine's default tolerance of 0.1, so where
//! an exact minimum is known analytically we assert against that instead and
//! merely check Combine agreement at its own printed precision.

use kestrel::{CompiledModel, Datacard, FitConfig};

/// `data/tutorials/statistical_routines_tutorial/datacard.txt`
const COUNTING: &str = "\
imax 1
jmax 1
kmax 3
-------
shapes * * FAKE
------
bin          bin1
observation     6
-----
bin            bin1    bin1
process         ggH      WW
process           0       1
rate            2.3     5.4
-------
lumi     lnN   1.01    1.01
xs_ggH   lnN   1.10       -
xs_WW    lnN      -    1.16
";

fn fit(src: &str, cfg: FitConfig) -> kestrel::FitResult {
    let dc = Datacard::parse(src).unwrap();
    let model = CompiledModel::compile(&dc).unwrap();
    kestrel::fit::fit(model, &cfg).unwrap()
}

/// With all nuisances at zero the stationary point is where the expected yield
/// equals the observation: 2.3 r + 5.4 = 6, so r = 0.6/2.3.
const TRUE_R: f64 = 0.6 / 2.3;

#[test]
fn counting_card_best_fit() {
    let res = fit(COUNTING, FitConfig { poi_range: (-10.0, 10.0), ..Default::default() });

    assert!(res.valid);
    assert!(
        (res.poi.value - TRUE_R).abs() < 1e-5,
        "r = {}, exact minimum {}",
        res.poi.value,
        TRUE_R
    );
    // Combine converged (tolerance 1e-6) gives 0.260866.
    assert!((res.poi.value - 0.260866).abs() < 1e-4, "vs converged Combine 0.260866");
    assert!((res.poi.error - 1.12).abs() < 5e-3, "HESSE error {} vs Combine 1.12", res.poi.error);

    // The nuisances have no pull here, so they sit at zero. Combine reports
    // ~1e-4 for these, which is its own convergence residual, not a pull.
    for p in res.params.iter().filter(|p| p.name != "r") {
        assert!(p.value.abs() < 1e-4, "{} = {} should be ~0", p.name, p.value);
        assert!((p.error - 1.0).abs() < 1e-3, "{} error {} should be ~1", p.name, p.error);
    }
}

#[test]
fn counting_card_minos() {
    let res = fit(COUNTING, FitConfig { poi_range: (-10.0, 10.0), ..Default::default() });
    let (lo, hi) = res.minos.expect("MINOS was requested");

    // Combine's published `-1.004/+1.265` is its default-tolerance answer. Run
    // with `--cminDefaultMinimizerTolerance 1e-6` it gives the values below,
    // which is the honest target. MINOS is a profile-likelihood crossing, so
    // matching it pins the NLL convention, error_def and bounds all at once.
    assert!((lo + 1.005001).abs() < 1e-4, "MINOS lower {} vs Combine -1.005001", lo);
    assert!((hi - 1.264853).abs() < 1e-4, "MINOS upper {} vs Combine +1.264853", hi);
    // And still consistent with the published, less-converged numbers.
    assert!((lo + 1.004).abs() < 2e-3 && (hi - 1.265).abs() < 2e-3);
    assert!(lo < 0.0 && hi > 0.0, "MINOS errors bracket the minimum");
}

#[test]
fn background_only_fit_freezes_the_poi() {
    let res = fit(
        COUNTING,
        FitConfig { poi_range: (-10.0, 10.0), background_only: true, ..Default::default() },
    );
    assert_eq!(res.poi.value, 0.0);
    assert!(res.minos.is_none());
    // With r = 0 the expectation is 5.4 against 6 observed, so the constrained
    // backgrounds must pull upwards.
    let ww = res.params.iter().find(|p| p.name == "xs_WW").unwrap();
    assert!(ww.value > 0.0, "xs_WW should pull up, got {}", ww.value);
}

#[test]
fn poi_range_does_not_move_the_minimum() {
    // The bounded-parameter transform differs between these, but the minimum
    // must not: it is interior to both.
    let a = fit(COUNTING, FitConfig { poi_range: (-10.0, 10.0), ..Default::default() });
    let b = fit(COUNTING, FitConfig { poi_range: (0.0, 20.0), ..Default::default() });
    assert!((a.poi.value - b.poi.value).abs() < 1e-3, "{} vs {}", a.poi.value, b.poi.value);
}

#[test]
fn asymmetric_lnn_is_not_symmetrised_when_it_matters() {
    // 0.9/1.15 is not reciprocal, so it must take the asymmetric interpolator
    // and give a different answer from the symmetric 1.15.
    let asym = fit(&COUNTING.replace("1.16", "0.9/1.15"), FitConfig::default());
    let sym = fit(&COUNTING.replace("1.16", "1.15"), FitConfig::default());
    assert!(
        (asym.poi.value - sym.poi.value).abs() > 1e-9,
        "asymmetric kappa collapsed to the symmetric case"
    );
}

#[test]
fn automcstats_on_a_counting_card_is_a_no_op() {
    // A counting card carries no per-bin MC statistical errors, so
    // `setupBinPars`' `sub_err <= 0` branch applies and no parameter is made.
    for line in ["* autoMCStats 0", "* autoMCStats -1"] {
        let dc = Datacard::parse(&format!("{COUNTING}\n{line}\n")).unwrap();
        let model = CompiledModel::compile(&dc).unwrap();
        assert!(model.bb_bins.is_empty(), "{line} should create no bins");
    }
}

#[test]
fn reciprocal_kappas_collapse_to_symmetric() {
    // Combine's addAsymmLogNormal degrades to symmetric when |kappa_lo*kappa_hi - 1| < 1e-5.
    let asym = fit(&COUNTING.replace("1.16", "0.862068965/1.16"), FitConfig::default());
    let sym = fit(&COUNTING.replace("1.16", "1.16"), FitConfig::default());
    assert!(
        (asym.poi.value - sym.poi.value).abs() < 1e-9,
        "{} vs {}",
        asym.poi.value,
        sym.poi.value
    );
}
