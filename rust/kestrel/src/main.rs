//! Native CLI. Mirrors Combine's own output format where it makes sense, so
//! that reference numbers can be diffed directly.

use clap::{Parser, Subcommand};
use kestrel::Datacard;

#[derive(Parser)]
#[command(name = "kestrel", about = "Combine-style maximum-likelihood fits")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse a datacard and dump the resulting AST as JSON.
    Parse { card: std::path::PathBuf },
    /// Parse every `*.txt` under a directory and report what succeeded.
    Sweep { dir: std::path::PathBuf },
    /// Dump per-bin expected yields as JSON, for cross-checking against
    /// `scripts/combine_reference.py`.
    Yields {
        card: std::path::PathBuf,
        #[arg(long, default_value = "120")]
        mass: String,
        #[arg(long)]
        ignore_shapes: bool,
        /// A reference JSON: evaluate at exactly its parameter points instead
        /// of only the pre-fit one.
        #[arg(long)]
        at: Option<std::path::PathBuf>,
    },
    /// Profile-likelihood scan of the POI.
    #[command(allow_negative_numbers = true)]
    Scan {
        card: std::path::PathBuf,
        #[arg(long, default_value_t = 0.0)]
        r_min: f64,
        #[arg(long, default_value_t = 20.0)]
        r_max: f64,
        #[arg(long, default_value_t = 50)]
        points: usize,
        #[arg(long, default_value = "120")]
        mass: String,
        #[arg(long)]
        ignore_shapes: bool,
    },
    /// Nuisance impacts, in the JSON layout `plotImpacts.py` reads.
    #[command(allow_negative_numbers = true)]
    Impacts {
        card: std::path::PathBuf,
        #[arg(long, default_value_t = 0.0)]
        r_min: f64,
        #[arg(long, default_value_t = 20.0)]
        r_max: f64,
        /// Symmetric HESSE errors on the nuisances instead of MINOS crossings.
        /// Much cheaper, and what Combine falls back to when a search fails.
        #[arg(long)]
        symmetric: bool,
        #[arg(long, default_value = "120")]
        mass: String,
        #[arg(long)]
        ignore_shapes: bool,
        /// Write `impacts.json` here as well as printing the table.
        #[arg(long)]
        json: Option<std::path::PathBuf>,
    },
    /// Fit a datacard, printing results in Combine's own format.
    #[command(allow_negative_numbers = true)]
    Fit {
        card: std::path::PathBuf,
        #[arg(long, default_value_t = 0.0)]
        r_min: f64,
        #[arg(long, default_value_t = 20.0)]
        r_max: f64,
        #[arg(long)]
        no_minos: bool,
        /// Value substituted for `$MASS` in the shapes patterns.
        #[arg(long, default_value = "120")]
        mass: String,
        /// Skip shape systematics entirely. The interval is then NOT comparable
        /// to Combine's; see the warning it prints.
        #[arg(long)]
        ignore_shapes: bool,
    },
}

/// Parse a card, load any templates it references, and compile the model.
fn build_model(
    card: &std::path::Path,
    mass: &str,
    ignore_shapes: bool,
) -> kestrel::Result<kestrel::CompiledModel> {
    let src = std::fs::read_to_string(card)
        .map_err(|e| kestrel::Error::Inconsistent(format!("cannot read {}: {e}", card.display())))?;
    let dc = kestrel::Datacard::parse(&src)?;
    let handling = if ignore_shapes {
        kestrel::ShapeHandling::Ignore
    } else {
        kestrel::ShapeHandling::Morph
    };

    if !dc.has_shapes() {
        return kestrel::CompiledModel::compile(&dc);
    }
    let dir = card.parent().unwrap_or(std::path::Path::new("."));
    let names = kestrel::shapes::referenced_files(&dc, mass);
    let mut files = kestrel::ShapeFiles::from_dir(dir, &names)?;
    let templates = kestrel::shapes::load_with(&dc, &mut files, mass, ignore_shapes)?;
    kestrel::CompiledModel::compile_with_shapes(&dc, &templates, handling)
}

fn main() -> std::process::ExitCode {
    match Cli::parse().cmd {
        Cmd::Parse { card } => {
            let src = std::fs::read_to_string(&card).expect("cannot read card");
            match Datacard::parse(&src) {
                Ok(dc) => {
                    println!("{}", serde_json::to_string_pretty(&dc).unwrap());
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{}: {}", card.display(), e);
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Cmd::Fit { card, r_min, r_max, no_minos, mass, ignore_shapes } => {
            let mut ignored = Vec::new();
            let run = (|| {
                let model = build_model(&card, &mass, ignore_shapes)?;
                ignored = model.ignored_shape_systs.clone();
                for w in &model.warnings {
                    eprintln!("warning: {w}");
                }
                let cfg = kestrel::FitConfig {
                    poi_range: (r_min, r_max),
                    minos: !no_minos,
                    ..Default::default()
                };
                kestrel::fit::fit(model, &cfg)
            })();
            if !ignored.is_empty() {
                eprintln!(
                    "warning: {} shape systematic{} ignored ({}{}).\n\
                     warning: the fit is LESS constrained than Combine's, so the interval is\n\
                     warning: narrower and NOT comparable to a full Combine result.",
                    ignored.len(),
                    if ignored.len() == 1 { "" } else { "s" },
                    ignored.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
                    if ignored.len() > 3 { ", ..." } else { "" },
                );
            }
            match run {
                Ok(res) => {
                    println!("\n --- kestrel ---");
                    match res.minos {
                        Some((lo, hi)) => println!(
                            "Best fit {}: {:.6}  {:+.6}/{:+.6}  (68% CL)",
                            res.poi.name, res.poi.value, lo, hi
                        ),
                        None => println!("Best fit {}: {:.6}", res.poi.name, res.poi.value),
                    }
                    println!("\n  Floating Parameter    FinalValue +/-  Error");
                    println!("--------------------  --------------------------");
                    for p in &res.params {
                        println!("{:>20}  {:>12.4e} +/- {:>8.2e}", p.name, p.value, p.error);
                    }
                    println!(
                        "\nedm = {:.3e}, nfcn = {}, cascade step = {}",
                        res.edm, res.nfcn, res.cascade_step
                    );
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{}: {}", card.display(), e);
                    std::process::ExitCode::FAILURE
                }
            }
        }
        Cmd::Scan { card, r_min, r_max, points, mass, ignore_shapes } => {
            let model = match build_model(&card, &mass, ignore_shapes) {
                Ok(m) => m,
                Err(e) => { eprintln!("{}: {}", card.display(), e); return std::process::ExitCode::FAILURE; }
            };
            let cfg = kestrel::FitConfig { poi_range: (r_min, r_max), ..Default::default() };
            match kestrel::scan(&model, &cfg, (r_min, r_max), points) {
                Ok(res) => {
                    println!("best fit r = {:.6}", res.best_fit);
                    println!("{:>12} {:>14} {:>7}", "r", "2*dNLL", "ok");
                    for p in &res.points {
                        println!("{:>12.6} {:>14.6} {:>7}", p.r, p.delta_nll, p.valid);
                    }
                    if let Some((a, b)) = res.crossings_68 {
                        println!("\n68% crossings: {:.6} .. {:.6}  ->  {:+.6}/{:+.6}",
                                 a, b, a - res.best_fit, b - res.best_fit);
                    }
                    if let Some((a, b)) = res.crossings_95 {
                        println!("95% crossings: {:.6} .. {:.6}", a, b);
                    }
                    std::process::ExitCode::SUCCESS
                }
                Err(e) => { eprintln!("{}: {}", card.display(), e); std::process::ExitCode::FAILURE }
            }
        }
        Cmd::Impacts { card, r_min, r_max, symmetric, mass, ignore_shapes, json } => {
            let model = match build_model(&card, &mass, ignore_shapes) {
                Ok(m) => m,
                Err(e) => { eprintln!("{}: {}", card.display(), e); return std::process::ExitCode::FAILURE; }
            };
            let cfg = kestrel::FitConfig { poi_range: (r_min, r_max), ..Default::default() };
            let res = match kestrel::fit::fit(model.clone(), &cfg) {
                Ok(r) => r,
                Err(e) => { eprintln!("{}: {}", card.display(), e); return std::process::ExitCode::FAILURE; }
            };
            let anchor = kestrel::Anchor::from_fit(&res);
            let which = kestrel::impacts::nuisances(&model);
            let mut out = kestrel::impacts_for(&model, &cfg, &anchor, &which, !symmetric);
            out.sort_by(|a, b| b.impact.total_cmp(&a.impact));

            println!("best fit {} = {:.6}", res.poi.name, res.poi.value);
            println!("{:>34} {:>12} {:>12} {:>12} {:>5}", "nuisance", "impact-", "impact+", "theta_hat", "ok");
            for i in &out {
                println!(
                    "{:>34} {:>12.6} {:>12.6} {:>12.6} {:>5}",
                    i.name, i.poi[0] - i.poi[1], i.poi[2] - i.poi[1], i.fit[1], i.ok
                );
            }
            if let Some(path) = json {
                let (lo, hi) = res.minos.unwrap_or((-res.poi.error, res.poi.error));
                let doc = serde_json::json!({
                    "POIs": [{ "name": res.poi.name,
                               "fit": [res.poi.value + lo, res.poi.value, res.poi.value + hi] }],
                    "method": "default",
                    "params": out.iter().map(|i| serde_json::json!({
                        "name": i.name,
                        "type": i.kind,
                        "groups": [],
                        "prefit": i.prefit,
                        "fit": i.fit,
                        res.poi.name.clone(): i.poi,
                        format!("impact_{}", res.poi.name): i.impact,
                        // Not part of Combine's schema; plotImpacts.py ignores
                        // it, and without it a shift we could not compute is
                        // indistinguishable from a genuine zero.
                        "converged": i.ok,
                    })).collect::<Vec<_>>(),
                });
                std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap())
                    .expect("cannot write the impacts JSON");
                eprintln!("wrote {}", path.display());
            }
            std::process::ExitCode::SUCCESS
        }
        Cmd::Yields { card, mass, ignore_shapes, at } => {
            let model = match build_model(&card, &mass, ignore_shapes) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{}: {}", card.display(), e);
                    return std::process::ExitCode::FAILURE;
                }
            };

            // Each entry is one parameter point, as name -> value. With no
            // reference we evaluate the single pre-fit point.
            let points: Vec<std::collections::HashMap<String, f64>> = match &at {
                None => vec![model.params.iter().map(|p| (p.name.clone(), p.init)).collect()],
                Some(path) => {
                    let raw = std::fs::read_to_string(path).expect("cannot read the reference");
                    let v: serde_json::Value = serde_json::from_str(&raw).expect("bad reference JSON");
                    v["points"]
                        .as_array()
                        .expect("reference has no `points`")
                        .iter()
                        .map(|p| serde_json::from_value(p["values"].clone()).unwrap())
                        .collect()
                }
            };

            let mut out = Vec::new();
            for values in &points {
                let par: Vec<f64> = model
                    .params
                    .iter()
                    .map(|p| *values.get(&p.name).unwrap_or(&p.init))
                    .collect();
                let mut exp = vec![0.0; model.n_bins];
                model.expected(&par, &mut exp, &mut Default::default());

                let expected: serde_json::Map<_, _> = model
                    .channels
                    .iter()
                    .map(|ch| {
                        let slice = &exp[ch.bin_off..ch.bin_off + ch.n_bins];
                        (ch.name.clone(), serde_json::json!(slice))
                    })
                    .collect();
                let norms: serde_json::Map<_, _> = model
                    .procs
                    .iter()
                    .map(|p| {
                        let key = format!("{}/{}", model.channels[p.channel].name, p.name);
                        (key, serde_json::json!(model.normalisation(p, &par)))
                    })
                    .collect();
                out.push(serde_json::json!({ "expected": expected, "norms": norms }));
            }

            let channels: serde_json::Map<_, _> = model
                .channels
                .iter()
                .map(|ch| (ch.name.clone(), serde_json::json!(ch.n_bins)))
                .collect();
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "channels": channels,
                    "nParams": model.params.len(),
                    "nMorphs": model.morphs.len(),
                    "morphsPerProcess": model.procs.iter()
                        .map(|p| serde_json::json!({"process": p.name, "n": p.morphs.len()}))
                        .collect::<Vec<_>>(),
                    "observed": model.obs,
                    "points": out,
                }))
                .unwrap()
            );
            std::process::ExitCode::SUCCESS
        }
        Cmd::Sweep { dir } => {
            let (mut ok, mut unsupported, mut failed) = (0, 0, 0);
            let mut entries: Vec<_> = walk(&dir);
            entries.sort();
            for path in entries {
                let src = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                // Cheap filter: a datacard always has these two anchors.
                if !src.contains("imax") || !src.contains("process") {
                    continue;
                }
                let rel = path.strip_prefix(&dir).unwrap_or(&path).display().to_string();
                match Datacard::parse(&src) {
                    Ok(dc) => {
                        ok += 1;
                        println!(
                            "  ok         {:<58} {} chan, {} col, {} syst{}",
                            rel,
                            dc.channels.len(),
                            dc.keyline.len(),
                            dc.systs.len(),
                            if dc.has_shapes() { ", shapes" } else { "" }
                        );
                    }
                    Err(e @ kestrel::Error::Unsupported { .. }) => {
                        unsupported += 1;
                        println!("  unsupported {:<57} {}", rel, e);
                    }
                    Err(e) => {
                        failed += 1;
                        println!("  FAILED     {:<58} {}", rel, e);
                    }
                }
            }
            println!("\n{} parsed, {} unsupported (named), {} failed", ok, unsupported, failed);
            if failed > 0 {
                std::process::ExitCode::FAILURE
            } else {
                std::process::ExitCode::SUCCESS
            }
        }
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|x| x == "txt") {
            out.push(p);
        }
    }
    out
}
