//! Datacard parser.
//!
//! `bin` appears twice with different meanings, so we locate the `observation`
//! and `rate` anchors first rather than parsing top to bottom. Systematics
//! lines carry a free-form trailing comment with no `#`, so surplus tokens are
//! discarded.

use super::ast::*;
use crate::error::{Error, Result};

/// A logical line: source line number, plus its whitespace-separated tokens.
struct Line {
    no: usize,
    tok: Vec<String>,
}

/// Strip comments and separator rules, and tokenise. Blank lines are dropped.
fn tokenise(src: &str) -> Vec<Line> {
    src.lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let body = raw.split('#').next().unwrap_or("");
            let tok: Vec<String> = body.split_whitespace().map(str::to_string).collect();
            // Separator rules are runs of dashes; they carry no information.
            if tok.is_empty() || tok.iter().all(|t| t.chars().all(|c| c == '-')) {
                return None;
            }
            Some(Line { no: i + 1, tok })
        })
        .collect()
}

/// `imax`/`jmax`/`kmax` accept `*`, meaning "infer from the card".
fn parse_max(line: &Line) -> Result<Option<usize>> {
    match line.tok.get(1).map(String::as_str) {
        None => Err(Error::syntax(line.no, format!("{} needs a value", line.tok[0]))),
        Some("*") => Ok(None),
        Some(v) => v
            .parse::<usize>()
            .map(Some)
            .map_err(|_| Error::syntax(line.no, format!("{:?} is not a count", v))),
    }
}

fn parse_f64(line: usize, s: &str, what: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| Error::syntax(line, format!("{} {:?} is not a number", what, s)))
}

/// A systematics column: one or more dashes means "no effect", as does an
/// explicit zero. `kappa_down/kappa_up` is the asymmetric form.
fn parse_effect(line: usize, s: &str) -> Result<Option<Effect>> {
    if s.chars().all(|c| c == '-') {
        return Ok(None);
    }
    if let Some((lo, hi)) = s.split_once('/') {
        let down = parse_f64(line, lo, "kappa")?;
        let up = parse_f64(line, hi, "kappa")?;
        return Ok(Some(Effect::Asym { down, up }));
    }
    let v = parse_f64(line, s, "kappa")?;
    Ok(if v == 0.0 { None } else { Some(Effect::Sym(v)) })
}

pub fn parse(src: &str) -> Result<Datacard> {
    let lines = tokenise(src);
    let kw = |l: &Line| l.tok[0].to_ascii_lowercase();

    let mut card = Datacard {
        imax: None,
        jmax: None,
        kmax: None,
        shape_lines: Vec::new(),
        channels: Vec::new(),
        observation: Vec::new(),
        keyline: Vec::new(),
        systs: Vec::new(),
        auto_mc_stats: Vec::new(),
    };

    // --- structural anchors -------------------------------------------------
    let i_obs = lines
        .iter()
        .position(|l| kw(l) == "observation")
        .ok_or_else(|| Error::Inconsistent("no `observation` line".into()))?;
    let i_rate = lines
        .iter()
        .position(|l| kw(l) == "rate")
        .ok_or_else(|| Error::Inconsistent("no `rate` line".into()))?;
    if i_rate < i_obs {
        return Err(Error::Inconsistent("`rate` appears before `observation`".into()));
    }

    // --- header -------------------------------------------------------------
    // `shapes` may sit either side of `observation`, so scan up to `rate`; only
    // a `bin` line *before* `observation` names the channels.
    for (i, l) in lines[..i_rate].iter().enumerate() {
        match kw(l).as_str() {
            "imax" => card.imax = parse_max(l)?,
            "jmax" => card.jmax = parse_max(l)?,
            "kmax" => card.kmax = parse_max(l)?,
            "shapes" => {
                if l.tok.len() < 4 {
                    return Err(Error::syntax(l.no, "shapes needs <process> <channel> <file>"));
                }
                // `shapes * * FAKE` promotes a counting card to shape form and
                // carries no histogram pattern.
                let hist = l.tok.get(4).cloned().unwrap_or_default();
                if hist.contains(':') {
                    return Err(Error::unsupported(
                        l.no,
                        format!("parametric shapes ({:?}); only TH1 histograms are supported", hist),
                    ));
                }
                card.shape_lines.push(ShapeLine {
                    process: l.tok[1].clone(),
                    channel: l.tok[2].clone(),
                    file: l.tok[3].clone(),
                    hist_pattern: hist,
                    syst_pattern: l.tok.get(5).cloned(),
                });
            }
            "bin" if i < i_obs => card.channels = l.tok[1..].to_vec(),
            _ => {}
        }
    }

    // --- observation --------------------------------------------------------
    let obs = &lines[i_obs];
    card.observation = obs.tok[1..]
        .iter()
        .map(|s| parse_f64(obs.no, s, "observation"))
        .collect::<Result<_>>()?;
    // --- the rate table -----------------------------------------------------
    // Between `observation` and `rate` sit one `bin` row and two `process`
    // rows. The signal/background row is the one that is entirely integers.
    let block = &lines[i_obs + 1..i_rate];
    let mut bin_row: Option<&Line> = None;
    let mut proc_names: Option<&Line> = None;
    let mut proc_codes: Option<&Line> = None;

    for l in block {
        match kw(l).as_str() {
            "bin" => bin_row = Some(l),
            "process" => {
                let all_int = l.tok[1..].iter().all(|t| t.parse::<i64>().is_ok());
                if all_int && proc_codes.is_none() {
                    proc_codes = Some(l);
                } else {
                    proc_names = Some(l);
                }
            }
            "shapes" => {}
            other => {
                return Err(Error::syntax(
                    l.no,
                    format!("unexpected {:?} between `observation` and `rate`", other),
                ))
            }
        }
    }

    let names = proc_names.ok_or_else(|| Error::Inconsistent("no process-name row".into()))?;
    let codes = proc_codes.ok_or_else(|| Error::Inconsistent("no process-code row".into()))?;
    let bins = bin_row.ok_or_else(|| Error::Inconsistent("no per-column `bin` row".into()))?;
    let rate = &lines[i_rate];

    // A card may omit the channel `bin` line entirely; recover the names from
    // the rate table instead, keeping first-appearance order.
    if card.channels.is_empty() {
        for c in &bins.tok[1..] {
            if !card.channels.contains(c) {
                card.channels.push(c.clone());
            }
        }
    }
    if card.channels.len() != card.observation.len() {
        return Err(Error::Inconsistent(format!(
            "{} channels but {} observations",
            card.channels.len(),
            card.observation.len()
        )));
    }

    let n = names.tok.len() - 1;
    for (label, l) in [("bin", bins), ("process", codes), ("rate", rate)] {
        if l.tok.len() - 1 != n {
            return Err(Error::Inconsistent(format!(
                "the `{}` row has {} columns but the process row has {}",
                label,
                l.tok.len() - 1,
                n
            )));
        }
    }

    for i in 0..n {
        let code: i64 = codes.tok[i + 1].parse().unwrap();
        card.keyline.push(KeyEntry {
            channel: bins.tok[i + 1].clone(),
            process: names.tok[i + 1].clone(),
            is_signal: code <= 0,
            rate: parse_f64(rate.no, &rate.tok[i + 1], "rate")?,
        });
    }

    // --- systematics --------------------------------------------------------
    for l in &lines[i_rate + 1..] {
        parse_syst_line(l, n, &mut card)?;
    }

    card.systs.retain(|s| !s.is_null());
    Ok(card)
}

/// One line of the systematics block. `n` is the number of rate-table columns.
fn parse_syst_line(l: &Line, n: usize, card: &mut Datacard) -> Result<()> {
    // `<channel> autoMCStats <threshold> [include-signal] [hist-mode]` puts the
    // channel first, so it is matched on the second token, not the first.
    if l.tok.len() >= 2 && l.tok[1] == "autoMCStats" {
        let threshold = parse_f64(l.no, l.tok.get(2).map(String::as_str).unwrap_or("0"), "threshold")?;
        card.auto_mc_stats.push((
            l.tok[0].clone(),
            AutoMcStats {
                threshold,
                include_signal: l.tok.get(3).map(|s| s != "0").unwrap_or(false),
                hist_mode: l.tok.get(4).and_then(|s| s.parse().ok()).unwrap_or(1),
            },
        ));
        return Ok(());
    }

    // Directives that are not nuisances at all.
    match l.tok[0].as_str() {
        "nuisance" => {
            return Err(Error::unsupported(l.no, "`nuisance edit` directives"));
        }
        "extArg" => return Err(Error::unsupported(l.no, "`extArg`")),
        _ => {}
    }
    if l.tok.len() >= 2 {
        match l.tok[1].as_str() {
            "group" => return Ok(()), // nuisance groups are freeze-sets; harmless to ignore
            "discrete" => return Err(Error::unsupported(l.no, "`discrete` (RooMultiPdf profiling)")),
            "extArg" => return Err(Error::unsupported(l.no, "`extArg`")),
            _ => {}
        }
    }

    let raw_name = l.tok[0].clone();
    let (name, nofloat) = match raw_name.strip_suffix("[nofloat]") {
        Some(base) => (base.to_string(), true),
        None => (raw_name, false),
    };
    let ty = l
        .tok
        .get(1)
        .ok_or_else(|| Error::syntax(l.no, format!("nuisance {:?} has no type", name)))?
        .clone();

    // Kinds carrying no per-column data.
    let kind = match ty.as_str() {
        "param" => {
            let mean = parse_f64(l.no, l.tok.get(2).map(String::as_str).unwrap_or("0"), "mean")?;
            let spec = l.tok.get(3).map(String::as_str).unwrap_or("1");
            let (lo, hi) = match spec.split_once('/') {
                // `-0.1/+0.2`: an asymmetric prior width.
                Some((a, b)) => (parse_f64(l.no, a, "sigma")?.abs(), parse_f64(l.no, b, "sigma")?.abs()),
                None => {
                    let s = parse_f64(l.no, spec, "sigma")?;
                    (s, s)
                }
            };
            card.systs.push(Systematic {
                name,
                nofloat,
                kind: SystKind::Param { mean, sigma_lo: lo, sigma_hi: hi },
                cols: Vec::new(),
                line: l.no,
            });
            return Ok(());
        }
        "flatParam" => {
            card.systs.push(Systematic {
                name,
                nofloat,
                kind: SystKind::FlatParam,
                cols: Vec::new(),
                line: l.no,
            });
            return Ok(());
        }
        "rateParam" => {
            if l.tok.len() < 5 {
                return Err(Error::syntax(l.no, "rateParam needs <bin> <process> <init>"));
            }
            let init_tok = &l.tok[4];
            // The formula form is `rateParam <bin> <proc> <expr> <args>`; the
            // expression is not a bare number.
            let init = init_tok.parse::<f64>().map_err(|_| {
                Error::Unsupported {
                    line: l.no,
                    what: format!("the formula form of `rateParam` ({:?})", init_tok),
                }
            })?;
            let range = l.tok.get(5).and_then(|r| {
                let t = r.trim_start_matches('[').trim_end_matches(']');
                let (a, b) = t.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            });
            card.systs.push(Systematic {
                name,
                nofloat,
                kind: SystKind::RateParam {
                    channel: l.tok[2].clone(),
                    process: l.tok[3].clone(),
                    init,
                    range,
                },
                cols: Vec::new(),
                line: l.no,
            });
            return Ok(());
        }
        "lnN" => SystKind::LnN,
        "lnU" => SystKind::LnU,
        "shape" => SystKind::Shape { log: false },
        "shapeN" => SystKind::Shape { log: true },
        "shapeU" => SystKind::ShapeU,
        "gmN" => {
            let v = l
                .tok
                .get(2)
                .ok_or_else(|| Error::syntax(l.no, "gmN needs its sideband count"))?;
            let n_obs = v
                .parse::<u32>()
                .map_err(|_| Error::syntax(l.no, format!("gmN count {:?} is not an integer", v)))?;
            SystKind::GmN { n_obs }
        }
        other => {
            return Err(Error::unsupported(
                l.no,
                format!("nuisance type {:?}", other),
            ))
        }
    };

    // `gmN` inserts its sideband count before the columns; everything else
    // starts at token 2. Trailing tokens are a free-form comment.
    let first = if matches!(kind, SystKind::GmN { .. }) { 3 } else { 2 };
    let avail = l.tok.len().saturating_sub(first);
    if avail < n {
        return Err(Error::Inconsistent(format!(
            "line {}: nuisance {:?} has {} columns but the card has {}",
            l.no, name, avail, n
        )));
    }
    let cols = l.tok[first..first + n]
        .iter()
        .map(|s| parse_effect(l.no, s))
        .collect::<Result<Vec<_>>>()?;

    card.systs.push(Systematic { name, nofloat, kind, cols, line: l.no });
    Ok(())
}
