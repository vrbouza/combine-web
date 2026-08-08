//! Resolving `shapes` lines and loading the TH1 templates they point at.

use std::collections::HashMap;

use oxyroot::RootFile;

use crate::datacard::{Datacard, SystKind};
use crate::error::{Error, Result};

/// The ROOT files a datacard references, keyed by the name written in the card.
/// In the browser these arrive as bytes; natively they are read from disk.
pub struct ShapeFiles {
    files: HashMap<String, RootFile>,
}

impl ShapeFiles {
    pub fn from_bytes(map: HashMap<String, Vec<u8>>) -> Result<Self> {
        let mut files = HashMap::new();
        for (name, bytes) in map {
            let f = RootFile::open_from_buffer(bytes)
                .map_err(|e| Error::Inconsistent(format!("could not read {:?}: {}", name, e)))?;
            files.insert(name, f);
        }
        Ok(ShapeFiles { files })
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn from_dir(dir: &std::path::Path, names: &[String]) -> Result<Self> {
        let mut map = HashMap::new();
        for name in names {
            let path = dir.join(name);
            let bytes = std::fs::read(&path)
                .map_err(|_| Error::MissingShapeFile(path.display().to_string()))?;
            map.insert(name.clone(), bytes);
        }
        Self::from_bytes(map)
    }

    /// `check_errors` is set for process templates, whose bin errors feed
    /// autoMCStats. It is off for `data_obs`, where only the counts are used —
    /// and where a NaN in `fSumw2` is not merely hypothetical:
    /// `simple-shapes-TH1_input.root` has one.
    fn hist(&mut self, file: &str, path: &str, check_errors: bool) -> Result<Hist> {
        let f = self
            .files
            .get_mut(file)
            .ok_or_else(|| Error::MissingShapeFile(file.to_string()))?;
        let h = f.get_th1(path).map_err(|_| Error::MissingHistogram {
            file: file.to_string(),
            path: path.to_string(),
        })?;

        // `rebinH1` copies every template into a `TH1F` (`ShapeTools.py:1194`),
        // so Combine sees single precision even for a TH1D on disk. Keeping our
        // extra digits would leave a ~1e-8 relative error on every yield.
        let f32ify = |v: &[f64]| v.iter().map(|x| *x as f32 as f64).collect::<Vec<f64>>();
        let values = f32ify(h.values());
        let errors = f32ify(&h.errors());
        let bad = values.iter().position(|v| !v.is_finite()).map(|i| ("bin content", i)).or_else(|| {
            check_errors
                .then(|| errors.iter().position(|v| !v.is_finite()).map(|i| ("bin error", i)))
                .flatten()
        });
        if let Some((what, i)) = bad {
            return Err(Error::Inconsistent(format!(
                "histogram {:?} in {:?} has a non-finite {} in bin {}",
                path,
                file,
                what,
                i + 1
            )));
        }
        Ok(Hist { values, errors })
    }
}

#[derive(Debug, Clone)]
pub struct Hist {
    pub values: Vec<f64>,
    pub errors: Vec<f64>,
}

impl Hist {
    pub fn integral(&self) -> f64 {
        self.values.iter().sum()
    }
}

/// Everything the model builder needs from the ROOT files.
#[derive(Debug, Default)]
pub struct Templates {
    /// Keyed by (channel, process).
    pub nominal: HashMap<(String, String), Hist>,
    /// Keyed by channel.
    pub data: HashMap<String, Hist>,
    pub bins_per_channel: HashMap<String, usize>,
    /// Up/Down variations, keyed by (channel, process, systematic).
    pub variations: HashMap<(String, String, String), (Hist, Hist)>,
}

/// Substitute the `$`-wildcards Combine understands. `$SYSTEMATIC` is left
/// alone here: the caller appends `Up`/`Down` before asking for it.
fn substitute(pattern: &str, channel: &str, process: &str, mass: &str) -> String {
    pattern
        .replace("$CHANNEL", channel)
        .replace("$PROCESS", process)
        .replace("$MASS", mass)
}

/// Read the nominal template for every (channel, process), the observed data,
/// and — unless `skip_variations` — the Up/Down template of every shape
/// systematic that touches a process.
pub fn load_with(
    dc: &Datacard,
    files: &mut ShapeFiles,
    mass: &str,
    skip_variations: bool,
) -> Result<Templates> {
    let mut out = load(dc, files, mass)?;
    if skip_variations {
        return Ok(out);
    }

    for (col, k) in dc.keyline.iter().enumerate() {
        let Some(line) = dc.shape_line_for(&k.channel, &k.process) else { continue };
        let Some(pattern) = &line.syst_pattern else { continue };
        let file = substitute(&line.file, &k.channel, &k.process, mass);
        let base = substitute(pattern, &k.channel, &k.process, mass);

        for s in &dc.systs {
            if !matches!(s.kind, SystKind::Shape { .. } | SystKind::ShapeU) {
                continue;
            }
            if !matches!(s.cols.get(col), Some(Some(_))) {
                continue;
            }
            // Combine appends Up/Down itself; the card never writes them.
            let up = files.hist(&file, &base.replace("$SYSTEMATIC", &format!("{}Up", s.name)), true)?;
            let down =
                files.hist(&file, &base.replace("$SYSTEMATIC", &format!("{}Down", s.name)), true)?;

            let want = out.bins_per_channel[&k.channel];
            if up.values.len() != want || down.values.len() != want {
                return Err(Error::Inconsistent(format!(
                    "systematic {:?} on {:?} has {}/{} bins but the channel has {}",
                    s.name,
                    k.process,
                    up.values.len(),
                    down.values.len(),
                    want
                )));
            }
            out.variations
                .insert((k.channel.clone(), k.process.clone(), s.name.clone()), (up, down));
        }
    }
    Ok(out)
}

/// Read the nominal template for every (channel, process) plus the observed
/// data. Shape *systematics* are not touched here.
pub fn load(dc: &Datacard, files: &mut ShapeFiles, mass: &str) -> Result<Templates> {
    let mut out = Templates::default();

    for channel in &dc.channels {
        let line = dc.shape_line_for(channel, "data_obs").ok_or_else(|| {
            Error::Inconsistent(format!("no `shapes` line covers data_obs in {:?}", channel))
        })?;
        // The filename takes wildcards too: `all_histograms_fps4_$CHANNEL.root`.
        let file = substitute(&line.file, channel, "data_obs", mass);
        let path = substitute(&line.hist_pattern, channel, "data_obs", mass);
        let h = files.hist(&file, &path, false)?;
        out.bins_per_channel.insert(channel.clone(), h.values.len());
        out.data.insert(channel.clone(), h);
    }

    for k in &dc.keyline {
        let line = dc.shape_line_for(&k.channel, &k.process).ok_or_else(|| {
            Error::Inconsistent(format!(
                "no `shapes` line covers process {:?} in channel {:?}",
                k.process, k.channel
            ))
        })?;
        let file = substitute(&line.file, &k.channel, &k.process, mass);
        let path = substitute(&line.hist_pattern, &k.channel, &k.process, mass);
        let h = files.hist(&file, &path, true)?;

        let want = out.bins_per_channel[&k.channel];
        if h.values.len() != want {
            return Err(Error::Inconsistent(format!(
                "{:?} in channel {:?} has {} bins but data_obs has {}",
                k.process,
                k.channel,
                h.values.len(),
                want
            )));
        }
        out.nominal.insert((k.channel.clone(), k.process.clone()), h);
    }

    Ok(out)
}

/// Which files a card references, with wildcards expanded over every
/// (channel, process) the card declares.
pub fn referenced_files(dc: &Datacard, mass: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut push = |chan: &str, proc: &str| {
        if let Some(l) = dc.shape_line_for(chan, proc) {
            if !l.is_fake() {
                v.push(substitute(&l.file, chan, proc, mass));
            }
        }
    };
    for c in &dc.channels {
        push(c, "data_obs");
    }
    for k in &dc.keyline {
        push(&k.channel, &k.process);
    }
    v.sort();
    v.dedup();
    v
}
