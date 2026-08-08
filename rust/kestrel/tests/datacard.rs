//! Parser tests. The cards are inlined so the suite is hermetic; each is a
//! verbatim excerpt of a real Combine card, cited by path.

use kestrel::datacard::{Effect, SystKind};
use kestrel::{Datacard, Error};

/// `data/tutorials/statistical_routines_tutorial/datacard.txt`, the card whose
/// fit result is the M4 gate.
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
lumi     lnN   1.01    1.01    # luminosity uncertainty
xs_ggH   lnN   1.10       -    # gg->H cross section
xs_WW    lnN      -    1.16    # WW cross section
";

/// `data/tutorials/shapes/simple-shapes-TH1.txt`.
const SHAPES: &str = "\
imax 1
jmax 1
kmax *
---------------
shapes * * simple-shapes-TH1_input.root $PROCESS $PROCESS_$SYSTEMATIC
---------------
bin bin1
observation 85
------------------------------
bin             bin1       bin1
process         signal     background
process         0          1
rate            10         100
--------------------------------
lumi     lnN    1.10       1.0
bgnorm   lnN    1.00       1.3
alpha  shape    -          1   uncertainty on the background shape
sigma  shape    0.5        -   the histogram is a 2 sigma shift
";

#[test]
fn counting_card() {
    let dc = Datacard::parse(COUNTING).unwrap();
    assert_eq!(dc.imax, Some(1));
    assert_eq!(dc.channels, ["bin1"]);
    assert_eq!(dc.observation, [6.0]);
    assert_eq!(dc.keyline.len(), 2);

    assert_eq!(dc.keyline[0].process, "ggH");
    assert!(dc.keyline[0].is_signal, "process code 0 is signal");
    assert_eq!(dc.keyline[0].rate, 2.3);
    assert!(!dc.keyline[1].is_signal, "process code 1 is background");
    assert_eq!(dc.keyline[1].rate, 5.4);

    // `shapes * * FAKE` must not make this look like a shape analysis.
    assert!(!dc.has_shapes());

    assert_eq!(dc.systs.len(), 3);
    let lumi = &dc.systs[0];
    assert_eq!(lumi.name, "lumi");
    assert_eq!(lumi.kind, SystKind::LnN);
    assert_eq!(lumi.cols, [Some(Effect::Sym(1.01)), Some(Effect::Sym(1.01))]);
    // The trailing free-form comment must not be read as a column.
    assert_eq!(dc.systs[1].cols, [Some(Effect::Sym(1.10)), None]);
    assert_eq!(dc.systs[2].cols, [None, Some(Effect::Sym(1.16))]);
}

#[test]
fn shape_card() {
    let dc = Datacard::parse(SHAPES).unwrap();
    assert!(dc.has_shapes());
    assert_eq!(dc.kmax, None, "`kmax *` means infer");

    let s = &dc.shape_lines[0];
    assert_eq!((s.process.as_str(), s.channel.as_str()), ("*", "*"));
    assert_eq!(s.hist_pattern, "$PROCESS");
    assert_eq!(s.syst_pattern.as_deref(), Some("$PROCESS_$SYSTEMATIC"));

    // `sigma shape 0.5` scales theta before morphing; the 0.5 is the effect,
    // not a kappa.
    let sigma = dc.systs.iter().find(|s| s.name == "sigma").unwrap();
    assert_eq!(sigma.kind, SystKind::Shape { log: false });
    assert_eq!(sigma.cols, [Some(Effect::Sym(0.5)), None]);
}

#[test]
fn asymmetric_and_gmn() {
    let src = COUNTING.replace(
        "xs_WW    lnN      -    1.16",
        "xs_WW    lnN      -    0.9/1.15\nWW_norm  gmN 4    -    1.35",
    );
    let dc = Datacard::parse(&src).unwrap();

    let ww = dc.systs.iter().find(|s| s.name == "xs_WW").unwrap();
    assert_eq!(ww.cols[1], Some(Effect::Asym { down: 0.9, up: 1.15 }));

    // gmN puts its sideband count before the columns.
    let norm = dc.systs.iter().find(|s| s.name == "WW_norm").unwrap();
    assert_eq!(norm.kind, SystKind::GmN { n_obs: 4 });
    assert_eq!(norm.cols, [None, Some(Effect::Sym(1.35))]);
}

#[test]
fn all_null_nuisance_is_dropped() {
    let src = COUNTING.replace("xs_WW    lnN      -    1.16", "xs_WW    lnN      -       -");
    let dc = Datacard::parse(&src).unwrap();
    assert!(dc.systs.iter().all(|s| s.name != "xs_WW"));
    assert_eq!(dc.systs.len(), 2);
}

#[test]
fn nofloat_suffix_is_stripped() {
    let src = COUNTING.replace("lumi     lnN", "lumi[nofloat] lnN");
    let dc = Datacard::parse(&src).unwrap();
    let lumi = dc.systs.iter().find(|s| s.name == "lumi").unwrap();
    assert!(lumi.nofloat);
}

#[test]
fn most_specific_shapes_line_wins() {
    // As in `data/ci/template-analysis_shapeInterp.txt`, where ggH/bbH override
    // the catch-all to pick up a mass-suffixed histogram.
    let src = SHAPES.replace(
        "shapes * * simple-shapes-TH1_input.root $PROCESS $PROCESS_$SYSTEMATIC",
        "shapes * * in.root $PROCESS $PROCESS_$SYSTEMATIC\n\
         shapes signal * in.root signal$MASS signal$MASS_$SYSTEMATIC",
    );
    let dc = Datacard::parse(&src).unwrap();
    assert_eq!(dc.shape_line_for("bin1", "signal").unwrap().hist_pattern, "signal$MASS");
    assert_eq!(dc.shape_line_for("bin1", "background").unwrap().hist_pattern, "$PROCESS");
}

#[test]
fn shapes_after_observation() {
    // `data/benchmarks/summer11/hwws.130/hwwof_0j_shape.txt` puts `shapes`
    // after `Observation` and omits the channel `bin` line entirely.
    let src = "\
imax 1
jmax 1
kmax *
Observation 247
shapes * * in.root histo_$PROCESS
bin 1 1
process ggH qqWW
process 0 1
rate 21.2 141.6
lumi lnN 1.06 1.06
";
    let dc = Datacard::parse(src).unwrap();
    assert_eq!(dc.channels, ["1"], "channel name recovered from the rate table");
    assert_eq!(dc.observation, [247.0]);
    assert!(dc.has_shapes());
}

#[test]
fn out_of_scope_constructs_are_named() {
    let cases = [
        ("shapes * * FAKE", "shapes * * in.root w:$PROCESS", "parametric"),
        ("lumi     lnN   1.01    1.01", "sf rateParam bin1 WW (@0*@1) a,b", "rateParam"),
        ("lumi     lnN   1.01    1.01", "idx discrete", "discrete"),
        ("lumi     lnN   1.01    1.01", "lumi trG 1.0 1.0", "trG"),
    ];
    for (from, to, want) in cases {
        let err = Datacard::parse(&COUNTING.replace(from, to)).unwrap_err();
        let Error::Unsupported { what, line } = &err else {
            panic!("{to:?} gave {err:?}, expected Unsupported");
        };
        assert!(what.contains(want), "{what:?} should name {want:?}");
        assert!(*line > 0, "the error must carry a line number");
    }
}

#[test]
fn column_count_mismatch_is_caught() {
    let src = COUNTING.replace("lumi     lnN   1.01    1.01", "lumi     lnN   1.01");
    assert!(matches!(Datacard::parse(&src), Err(Error::Inconsistent(_))));
}
