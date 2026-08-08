//! Template loading and multi-bin fits.
//!
//! `tests/data/` holds verbatim copies of Combine's `simple-shapes-TH1.txt` and
//! its 9 kB input file, so the suite stays hermetic.

use std::collections::HashMap;
use std::path::PathBuf;

use kestrel::{CompiledModel, Datacard, ShapeFiles, ShapeHandling};

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn load() -> (Datacard, kestrel::Templates) {
    let dir = data_dir();
    let src = std::fs::read_to_string(dir.join("simple-shapes-TH1.txt")).unwrap();
    let dc = Datacard::parse(&src).unwrap();
    let names = kestrel::shapes::referenced_files(&dc, "120");
    let mut files = ShapeFiles::from_dir(&dir, &names).unwrap();
    let t = kestrel::shapes::load_with(&dc, &mut files, "120", false).unwrap();
    (dc, t)
}

/// Verified against uproot: the file's own integrals.
const SIGNAL_INTEGRAL: f64 = 10.000000040;
const BACKGROUND_INTEGRAL: f64 = 100.000002265;

#[test]
fn templates_load_with_the_right_shape() {
    let (_, t) = load();
    assert_eq!(t.bins_per_channel["bin1"], 10);
    assert_eq!(t.data["bin1"].values.iter().sum::<f64>(), 85.0);

    let sig = &t.nominal[&("bin1".into(), "signal".into())];
    let bkg = &t.nominal[&("bin1".into(), "background".into())];
    assert!((sig.integral() - SIGNAL_INTEGRAL).abs() < 1e-6);
    assert!((bkg.integral() - BACKGROUND_INTEGRAL).abs() < 1e-6);
    assert_eq!(sig.values.len(), 10);
}

#[test]
fn prefit_yields_use_the_card_rate_not_the_integral() {
    // Combine normalises to the `rate` line and takes only the shape from the
    // histogram. The two differ here in the eighth digit, so this distinguishes
    // the two conventions.
    let (dc, t) = load();
    let model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Ignore).unwrap();
    let par: Vec<f64> = model.params.iter().map(|p| p.init).collect();
    let mut expected = vec![0.0; model.n_bins];
    model.expected(&par, &mut expected, &mut Default::default());

    let sig = &t.nominal[&("bin1".into(), "signal".into())];
    let bkg = &t.nominal[&("bin1".into(), "background".into())];
    for b in 0..10 {
        let want = 10.0 * sig.values[b] / sig.integral() + 100.0 * bkg.values[b] / bkg.integral();
        assert!(
            (expected[b] - want).abs() < 1e-12,
            "bin {b}: {} vs {want}",
            expected[b]
        );
    }
    // Total is the sum of the card rates, not of the integrals.
    assert!((expected.iter().sum::<f64>() - 110.0).abs() < 1e-12);
}

#[test]
fn observation_comes_from_the_data_histogram_per_bin() {
    let (dc, t) = load();
    let model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Ignore).unwrap();
    assert_eq!(model.obs, t.data["bin1"].values);
    assert_eq!(model.obs.iter().sum::<f64>(), 85.0);
}

#[test]
fn ignore_mode_drops_shape_systematics_and_reports_them() {
    let (dc, t) = load();
    let model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Ignore).unwrap();

    let mut ignored = model.ignored_shape_systs.clone();
    ignored.sort();
    assert_eq!(ignored, ["alpha", "sigma"]);

    // Their parameters must not exist at all.
    let names: Vec<&str> = model.params.iter().map(|p| p.name.as_str()).collect();
    assert!(!names.contains(&"alpha"));
    assert!(!names.contains(&"sigma"));
    assert_eq!(names, ["r", "lumi", "bgnorm"]);
}

#[test]
fn morph_mode_builds_the_morphing() {
    let (dc, t) = load();
    // Both variations of both systematics must have been read.
    assert_eq!(t.variations.len(), 2);
    assert!(t.variations.contains_key(&("bin1".into(), "background".into(), "alpha".into())));
    assert!(t.variations.contains_key(&("bin1".into(), "signal".into(), "sigma".into())));

    let model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Morph).unwrap();
    assert!(model.ignored_shape_systs.is_empty());
    assert_eq!(model.morphs.len(), 2, "one morph per (process, systematic)");
    assert_eq!(model.morph_sum.len(), 2 * 10);

    // `alpha` scales the background integral by 1.15/0.90, so it must also
    // contribute an asymmetric log-normal. `sigma` preserves the integral to
    // eight digits, so Combine's 1e-3 skip drops its normalisation entirely.
    let bkg = model.procs.iter().find(|p| p.name == "background").unwrap();
    let sig = model.procs.iter().find(|p| p.name == "signal").unwrap();
    assert_eq!(bkg.asym_kappa.len(), 1, "alpha carries normalisation");
    assert!(sig.asym_kappa.is_empty(), "sigma is norm-preserving, so it is skipped");
    assert!(sig.log_kappa.iter().all(|(i, _)| model.params[*i].name != "sigma"));
}

#[test]
fn morphing_moves_the_yields_in_the_right_direction() {
    let (dc, t) = load();
    let model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Morph).unwrap();
    let idx = |n: &str| model.params.iter().position(|p| p.name == n).unwrap();

    let mut par: Vec<f64> = model.params.iter().map(|p| p.init).collect();
    let mut exp = vec![0.0; model.n_bins];
    let mut scratch = kestrel::model::EvalScratch::default();
    model.expected(&par, &mut exp, &mut scratch);
    let nominal: f64 = exp.iter().sum();
    assert!((nominal - 110.0).abs() < 1e-9);

    // alpha at +1 sigma scales the background integral by kappa_up = 1.15.
    par[idx("alpha")] = 1.0;
    model.expected(&par, &mut exp, &mut scratch);
    assert!((exp.iter().sum::<f64>() - 125.0).abs() < 1e-6, "got {}", exp.iter().sum::<f64>());

    // sigma is a pure resolution smear: it must move bins without moving the total.
    par[idx("alpha")] = 0.0;
    par[idx("sigma")] = 1.0;
    model.expected(&par, &mut exp, &mut scratch);
    assert!((exp.iter().sum::<f64>() - 110.0).abs() < 1e-6);
    assert!((exp[0] - 27.276178).abs() > 1e-6, "the shape must actually change");
}

#[test]
fn a_counting_card_with_a_shape_systematic_is_refused() {
    let src = std::fs::read_to_string(data_dir().join("simple-shapes-TH1.txt"))
        .unwrap()
        .replace("shapes * * simple-shapes-TH1_input.root $PROCESS $PROCESS_$SYSTEMATIC", "shapes * * FAKE");
    let dc = Datacard::parse(&src).unwrap();
    let err = CompiledModel::compile(&dc).unwrap_err();
    assert!(format!("{err}").contains("no templates"), "{err}");
}

#[test]
fn a_one_bin_shape_card_equals_the_counting_equivalent() {
    // The strongest internal check available without Combine: collapse the
    // template to a single bin and the answer must match a counting card with
    // the same rates.
    let dir = data_dir();
    let src = std::fs::read_to_string(dir.join("simple-shapes-TH1.txt")).unwrap();
    let dc = Datacard::parse(&src).unwrap();
    let names = kestrel::shapes::referenced_files(&dc, "120");
    let mut files = ShapeFiles::from_dir(&dir, &names).unwrap();
    let mut t = kestrel::shapes::load(&dc, &mut files, "120").unwrap();

    // Collapse every histogram to one bin holding its integral.
    for h in t.nominal.values_mut().chain(t.data.values_mut()) {
        let s = h.integral();
        h.values = vec![s];
        h.errors = vec![h.errors.iter().map(|e| e * e).sum::<f64>().sqrt()];
    }
    *t.bins_per_channel.get_mut("bin1").unwrap() = 1;

    let shape_model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Ignore).unwrap();
    let shape_fit = kestrel::fit::fit(shape_model, &Default::default()).unwrap();

    let counting = "\
imax 1
jmax 1
kmax 2
------
bin          bin1
observation     85
-----
bin            bin1       bin1
process        signal     background
process          0          1
rate            10        100
-------
lumi     lnN   1.10       1.0
bgnorm   lnN   1.00       1.3
";
    let cdc = Datacard::parse(counting).unwrap();
    let cmodel = CompiledModel::compile(&cdc).unwrap();
    let cfit = kestrel::fit::fit(cmodel, &Default::default()).unwrap();

    assert!(
        (shape_fit.poi.value - cfit.poi.value).abs() < 1e-7,
        "one-bin shape {} vs counting {}",
        shape_fit.poi.value,
        cfit.poi.value
    );
}

#[test]
fn missing_shape_file_is_named() {
    let dc = Datacard::parse(
        &std::fs::read_to_string(data_dir().join("simple-shapes-TH1.txt")).unwrap(),
    )
    .unwrap();
    let mut files = ShapeFiles::from_bytes(HashMap::new()).unwrap();
    let err = kestrel::shapes::load(&dc, &mut files, "120").unwrap_err();
    assert!(
        matches!(&err, kestrel::Error::MissingShapeFile(f) if f.contains("simple-shapes-TH1_input")),
        "{err:?}"
    );
}

/// `data/ci/template-analysis_shapeInterp_womcstats.txt`: 1 channel, 18 bins,
/// 7 processes, 53 shape systematics, 73 parameters. The flagship morphing
/// case.
///
/// The expected values are from real Combine 11.0.0 run on *this* card at
/// tolerance 1e-6. They are deliberately not the committed
/// `test/references/cmshistfunc.out` numbers, which belong to the variant
/// *with* `autoMCStats` and are a full 14% wider. See PRECISION.md.
#[test]
fn shape_morphing_reproduces_combine() {
    let dir = data_dir();
    let src = std::fs::read_to_string(dir.join("template-analysis_shapeInterp_womcstats.txt")).unwrap();
    let dc = Datacard::parse(&src).unwrap();
    let names = kestrel::shapes::referenced_files(&dc, "200");
    let mut files = ShapeFiles::from_dir(&dir, &names).unwrap();
    let t = kestrel::shapes::load_with(&dc, &mut files, "200", false).unwrap();
    let model = CompiledModel::compile_with_shapes(&dc, &t, ShapeHandling::Morph).unwrap();

    assert_eq!(model.params.len(), 73, "1 POI + 17 lnN + 53 shape + 2 rateParam");
    assert_eq!(model.morphs.len(), 69, "one morph per (systematic, process) entry");

    let cfg = kestrel::FitConfig { poi_range: (-5.0, 5.0), ..Default::default() };
    let res = kestrel::fit::fit(model, &cfg).unwrap();
    let (lo, hi) = res.minos.unwrap();

    assert!((res.poi.value - 0.616807).abs() < 1e-4, "r = {}", res.poi.value);
    assert!((lo + 0.519309).abs() < 1e-4, "MINOS lower = {lo}");
    assert!((hi - 0.524768).abs() < 1e-4, "MINOS upper = {hi}");
}
