# combine-wasm

[CMS Combine](https://github.com/cms-analysis/HiggsAnalysis-CombinedLimit)-style maximum-likelihood fits, run entirely in the browser via WebAssembly. Drop in a
datacard (and its shape `.root` file), pick a few parameters, and the fit runs client-side — no server,
no upload, no ROOT installation.

This project is a quick example on how to use the [sparrowhawk](https://github.com/bacpop/sparrowhawk) project as example and guide for developing or (as in this case) adapting a scientific methodology that can work with WebAssemblym with the help of LLMs for the initial set up. Thus, the code here is mostly vibe-coded using Claude Code.

**This is not a port of Combine.** Combine is ~41k lines of C++ built on ROOT, RooFit, RooStats, and
Minuit2, and ROOT cannot be compiled to WebAssembly: its `libCling` JIT is architecturally incompatible
with the wasm sandbox, and `libCore` depends on it. What this project provides is a re-implementation of
the subset of Combine's model and fit that a simple single-POI binned analysis needs, validated against
Combine's own published reference outputs.

This is also possible thanks to the nice port of Minuit2 to Rust of [minuit-rs](https://github.com/ricardofrantz/minuit-rs), and the ROOT-in-Rust [oxyroot](https://github.com/m-dupont/oxyroot) crate's code, that has been extended here to include some histogram functionality.

## Quick start
You'll need the Rust toolchain, and npm installed. With those,


```sh
git clone https://github.com/vrbouza/combine-wasm.git
cd www && npm install
npm run serve     # http://localhost:8080
```
Then, enter http://localhost:8080. You'll find some test datacards inside `rust/kestrel/test/data`. You can provide your own as well, but keep in mind that it only supports one POI.

## Layout

| Path | Contents |
|---|---|
| `rust/kestrel/` | The core library: datacard parser, model builder, likelihood, fit driver, wasm interface |
| `rust/oxyroot/` | Vendored fork of the pure-Rust ROOT reader, extended with TH1 streamers |
| `www/` | Vue 3 + TypeScript front end |
| `scripts/` | Reference-value generators (pyROOT, pyhf, uproot) used by the test suite |

## Scope

Supported: counting datacards and TH1 shape datacards, with `lnN`, `lnU`, `gmN`, `shape`/`shapeN`,
`param`, `rateParam` (numeric form), `flatParam`, and `autoMCStats`.

Not supported, each rejected with a message naming the construct and its line: parametric/unbinned cards
(`shapes ... w:$PROCESS`), multi-POI physics models, the formula form of `rateParam`, `extArg`,
`discrete`, and `nuisance edit` directives.

## Status

**Counting datacards fit end to end and reproduce Combine's reference numbers.** On
`data/tutorials/statistical_routines_tutorial/datacard.txt` with `r` in [-10, 10]:

| | kestrel | Combine |
|---|---|---|
| best-fit `r` | 0.260874 | +0.260 |
| HESSE error | 1.12 | 1.12 |
| MINOS | -1.0041/+1.2655 | -1.004/+1.265 |

The exact minimum here is `0.6/2.3 = 0.2608696`, so kestrel lands 4.4e-6 away. Note that Combine's own
published values are converged only to its default tolerance of 0.1 — its nuisances sit at ~1e-4 rather
than zero, which is convergence residual rather than a pull. kestrel defaults to a tolerance of 1e-4.

**Shape datacards fit with full vertical template morphing, validated against real Combine 11.0.0.**
On `template-analysis_shapeInterp_womcstats.txt` — 1 channel, 18 bins, 7 processes, 53 shape
systematics, 73 parameters:

| | `r` | MINOS |
|---|---|---|
| Combine 11.0.0 | 0.616807 | −0.519309 / +0.524768 |
| kestrel | 0.616808 | −0.519308 / +0.524765 |

Agreement to ~1e-6. Per-bin expected yields are checked independently by
`scripts/combine_reference.py --check`, which dumps yields from a real workspace at a dozen parameter
points; `simple-shapes-TH1.txt` matches to 2.1e-14.

Getting there required replicating two places where **Combine itself loses precision** — it quantises
shape-derived κ to six decimal places, and downcasts every template to `TH1F` during rebinning.

`simple-shapes-TH1.txt` likewise agrees to 7e-6 (`r = 0.176566` against Combine's `0.176571`).

Beyond the fit, the tool produces a **profile-likelihood scan** and a **correlation matrix** — plotted
as plain SVG and downloadable as JSON. Scan points are warm started from their neighbour outwards from
the minimum, which is needed for convergence and not merely for speed; MINOS falls back to the scan
when its own crossing search fails, as Combine's `--robustFit` does.

The scan is not limited to the POI. **Clicking a row in the impacts plot profiles that nuisance
instead**, over ±3 of its pre-fit width — so a nuisance the data cannot constrain draws its own prior,
a parabola crossing 68% at exactly ±1, and a constrained one is visibly narrower. That makes the
constraint column readable straight off the curve, and the curves comparable between rows. Clicking the
`r̂` readout goes back to the POI. Each nuisance profile is computed on demand and cached.

It also produces a full **pulls-and-impacts plot** in the style of `plotImpacts.py`. The impact of a
nuisance is the shift in the POI when that nuisance is fixed at its post-fit ±1σ and everything else,
the POI included, is re-profiled — a measure of correlation, not a decomposition of the uncertainty.
Errors come from a MINOS crossing search per nuisance, as Combine's default does, falling back to the
symmetric HESSE error where a search fails. On `simple-shapes-TH1` every impact matches Combine to
8.7e-6; on the 72-nuisance `template-analysis_shapeInterp_womcstats` card, 69 of the 70 comparable rows
agree to 6.7e-5, and the remaining three are cases where Combine's own output is degenerate or missing. A row whose shift had to be approximated is marked rather
than dropped, because an uncomputed shift must not read as a negligible one.

Because each nuisance is independent once the best fit is known, impacts run across a **pool of web
workers**, one wasm instance each — deliberately not wasm threads, which would need `SharedArrayBuffer`
and so COOP/COEP headers that break plain static hosting. On a 72-nuisance card that is 5.3 s on one
worker against 1.9 s on eight; a slider sets the pool size. Results download in Combine's own
`impacts.json` layout, so `plotImpacts.py` will render them unmodified.

Impacts are computed by default. Note that **impact, constraint and pull are three independent
things** — a nuisance can move a long way and leave the POI untouched, or sit exactly where its prior
put it and still dominate the uncertainty. The "Reading" tab in the interface spells out the three
definitions, including the two different quantities Combine calls the pull.

A rate-only mode (`--ignore-shapes`) skips shape systematics entirely, with a warning; it is validated
separately by `scripts/check_rate_only.py`.

**Known limitation:** channels using `autoMCStats` are fitted with Barlow-Beeston-lite, but Combine
morphs those channels with `CMSHistFunc` rather than `FastVerticalInterpHistPdf2`, and the two
interpolate differently. Such cards agree to ~1e-3 rather than ~1e-6 and the tool says so.

The parser reads 166 of Combine's datacards across `data/tutorials`, `data/ci` and `data/benchmarks`;
the 46 it rejects are all out-of-scope constructs, named individually.

**It runs in a browser.** `www/` is a Vue 3 + TypeScript app that loads the wasm in a Web Worker; drop in
a datacard and any `.root` files it names, and the fit runs client-side. The compiled wasm reproduces the
native numbers exactly (counting `r = 0.260874`; `simple-shapes` MINOS agrees to 1e-6).

Cards requesting `autoMCStats` are refused rather than mis-fitted: Combine builds one Barlow-Beeston
nuisance per bin for those, and dropping them silently moved the fitted `r` by 7% on
`templ_datacard_largeYields.txt`.

```sh
cd www && npm install
npm run serve     # http://localhost:8080
npm run build     # or a production bundle in dist/
```

The first start runs wasm-pack before webpack, so it takes about 30 seconds to reach "App running at".
Following sparrowhawk, `vue.config.js` disables hot reload and file watching — otherwise webpack
watches the Rust crate and the wasm rebuild loop never settles — so **reload the page by hand after an
edit**.

The front end follows sparrowhawk's design language: Tailwind 4 configured through PostCSS with no
`tailwind.config.js`, DM Sans on a `#F5F7F9` page, white cards bordered rather than shadowed, and its
`button`/`tooltip`/`tabs`/`collapsible` primitives copied in verbatim so markup moves between the two
projects unchanged.

In progress: vertical template morphing (so shape systematics stop being ignored), autoMCStats, the
profile-likelihood scan, and the plots.

## Building

```sh
cd rust/kestrel
cargo test                                        # native tests
cargo build --lib --target wasm32-unknown-unknown # must stay green; --lib, since the
                                                  # CLI needs clap, which is not a wasm dep
cargo run --bin kestrel -- parse <datacard>       # dump a parsed card as JSON
cargo run --bin kestrel -- sweep <dir>            # parse every card under a directory
cargo run --bin kestrel -- impacts <datacard>     # nuisance impacts, --json for Combine's layout
```

Validation against a real Combine install lives in `scripts/`. Note that
`scripts/combine_impacts.py` drives `combine` directly rather than going through
`combineTool.py -M Impacts`: the latter calls `prefit_from_workspace` before dispatching any
fit, and that raises on every card with an unconstrained parameter, because it compares a C++
null pointer to `None` and modern cppyy refuses. The script also does its own collection for
the same reason.

Requires Rust 1.88 or newer. Note that the `minuit2` dependency declares `rust-version = "1.85"` but
actually needs 1.88, since it uses `let` chains and trait-upcasting coercion.

