//! The browser-facing interface. Kept behind `cfg(target_family = "wasm")` so
//! the core stays free of web dependencies and lifts out as a plain library.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::{CompiledModel, Datacard, FitConfig, ShapeFiles, ShapeHandling};

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub r_min: f64,
    pub r_max: f64,
    pub mass: String,
    pub minos: bool,
    /// Skip shape systematics. The interval is then not comparable to Combine's.
    pub ignore_shapes: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            r_min: 0.0,
            r_max: 20.0,
            mass: "120".into(),
            minos: true,
            ignore_shapes: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    channels: Vec<ChannelSummary>,
    n_params: usize,
    n_bins: usize,
    /// Shape systematics skipped because `ignoreShapes` was set.
    ignored_shape_systs: Vec<String>,
    expected: Vec<f64>,
    observed: Vec<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSummary {
    name: String,
    n_bins: usize,
    processes: Vec<ProcessSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Nuisance {
    index: usize,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSummary {
    name: String,
    is_signal: bool,
    rate: f64,
}

#[wasm_bindgen]
pub struct WasmFitter {
    model: Option<CompiledModel>,
    cfg: Config,
    last: Option<crate::FitResult>,
}

#[wasm_bindgen]
impl WasmFitter {
    /// `shapes` maps the filename written in the datacard to its bytes; a card
    /// may name several files, and wasm cannot open them itself.
    #[wasm_bindgen(constructor)]
    pub fn new(datacard: &str, shapes: js_sys::Map, cfg: JsValue) -> Result<WasmFitter, JsValue> {
        let cfg: Config = if cfg.is_undefined() || cfg.is_null() {
            Config::default()
        } else {
            serde_wasm_bindgen::from_value(cfg).map_err(js_err)?
        };

        let dc = Datacard::parse(datacard).map_err(js_err)?;
        let handling = if cfg.ignore_shapes {
            ShapeHandling::Ignore
        } else {
            ShapeHandling::Morph
        };

        let model = if dc.has_shapes() {
            let mut map = HashMap::new();
            for key in shapes.keys() {
                let key = key?;
                let name = key.as_string().ok_or_else(|| js_err("shape keys must be strings"))?;
                let bytes = js_sys::Uint8Array::new(&shapes.get(&key)).to_vec();
                map.insert(name, bytes);
            }
            let mut files = ShapeFiles::from_bytes(map).map_err(js_err)?;
            let templates = crate::shapes::load_with(&dc, &mut files, &cfg.mass, cfg.ignore_shapes)
                .map_err(js_err)?;
            CompiledModel::compile_with_shapes(&dc, &templates, handling).map_err(js_err)?
        } else {
            CompiledModel::compile(&dc).map_err(js_err)?
        };

        Ok(WasmFitter { model: Some(model), cfg, last: None })
    }

    /// Which files the card references, with wildcards expanded. Call this
    /// before the constructor so the page knows what to ask the user for.
    #[wasm_bindgen(js_name = requiredFiles)]
    pub fn required_files(datacard: &str, mass: &str) -> Result<JsValue, JsValue> {
        let dc = Datacard::parse(datacard).map_err(js_err)?;
        serde_wasm_bindgen::to_value(&crate::shapes::referenced_files(&dc, mass)).map_err(js_err)
    }

    fn model(&self) -> Result<&CompiledModel, JsValue> {
        self.model.as_ref().ok_or_else(|| js_err("the model has been freed"))
    }

    /// The model without fitting: channels, processes, and pre-fit yields.
    pub fn summary(&self) -> Result<JsValue, JsValue> {
        let m = self.model()?;
        let par: Vec<f64> = m.params.iter().map(|p| p.init).collect();
        let mut expected = vec![0.0; m.n_bins];
        m.expected(&par, &mut expected, &mut Default::default());

        let channels = m
            .channels
            .iter()
            .map(|ch| ChannelSummary {
                name: ch.name.clone(),
                n_bins: ch.n_bins,
                processes: m.procs[ch.procs.clone()]
                    .iter()
                    .map(|p| ProcessSummary {
                        name: p.name.clone(),
                        is_signal: p.is_signal,
                        rate: p.nominal,
                    })
                    .collect(),
            })
            .collect();

        serde_wasm_bindgen::to_value(&Summary {
            channels,
            n_params: m.params.len(),
            n_bins: m.n_bins,
            ignored_shape_systs: m.ignored_shape_systs.clone(),
            expected,
            observed: m.obs.clone(),
        })
        .map_err(js_err)
    }

    /// Best fit with HESSE errors and, if enabled, MINOS asymmetric errors.
    pub fn fit(&mut self) -> Result<JsValue, JsValue> {
        let model = self.model.take().ok_or_else(|| js_err("the model has been freed"))?;
        let cfg = FitConfig {
            poi_range: (self.cfg.r_min, self.cfg.r_max),
            minos: self.cfg.minos,
            ..Default::default()
        };
        let res = crate::fit::fit(model.clone(), &cfg);
        self.model = Some(model);
        let res = res.map_err(js_err)?;
        self.last = Some(res.clone());
        serde_wasm_bindgen::to_value(&res).map_err(js_err)
    }

    /// Caveats the interface must show; empty when the fit is reference-grade.
    pub fn warnings(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.model()?.warnings).map_err(js_err)
    }

    /// Nuisance pulls and the correlation matrix from the last `fit()`.
    pub fn report(&self) -> Result<JsValue, JsValue> {
        let res = self.last.as_ref().ok_or_else(|| js_err("call fit() first"))?;
        serde_wasm_bindgen::to_value(&crate::report(self.model()?, res)).map_err(js_err)
    }

    /// Profile-likelihood scan of the POI. Runs as one call because each point
    /// is warm started from its neighbour, which chunking would break; the
    /// worker keeps the page responsive regardless.
    pub fn scan(&mut self, points: usize) -> Result<JsValue, JsValue> {
        let model = self.model()?;
        let cfg = crate::FitConfig {
            poi_range: (self.cfg.r_min, self.cfg.r_max),
            minos: false,
            ..Default::default()
        };
        let res = crate::scan(model, &cfg, (self.cfg.r_min, self.cfg.r_max), points)
            .map_err(js_err)?;
        serde_wasm_bindgen::to_value(&res).map_err(js_err)
    }

    /// Profile likelihood of any one parameter, over +-3 pre-fit sigma.
    ///
    /// The window is in prior units on purpose: a nuisance the data cannot
    /// constrain then reproduces its own prior, crossing 1 at exactly +-1, so
    /// the curve shows the constraint directly and is comparable between rows.
    #[wasm_bindgen(js_name = scanParam)]
    pub fn scan_param(&mut self, index: usize, points: usize) -> Result<JsValue, JsValue> {
        let last = self.last.as_ref().ok_or_else(|| js_err("call fit() first"))?;
        let m = self.model()?;
        let p = m.params.get(index).ok_or_else(|| js_err("no such parameter"))?;

        let range = if index == m.poi {
            (self.cfg.r_min, self.cfg.r_max)
        } else {
            match crate::report::prior_band(&p.kind) {
                Some((lo, hi)) => (p.init - 3.0 * lo, p.init + 3.0 * hi),
                // `flatParam`, `rateParam` and `lnU` have no prior width to
                // scale by, so fall back to the post-fit error.
                None => {
                    let c = last.params[index].value;
                    let s = last.params[index].error.abs().max(1e-6);
                    (c - 3.0 * s, c + 3.0 * s)
                }
            }
        };

        let cfg = FitConfig {
            poi_range: (self.cfg.r_min, self.cfg.r_max),
            minos: false,
            ..Default::default()
        };
        let res = crate::scan_param(m, &cfg, index, range, points).map_err(js_err)?;
        serde_wasm_bindgen::to_value(&res).map_err(js_err)
    }

    /// Parameters an impact can be computed for, as `{index, name}`. The index
    /// is what `impactsFor` slices on.
    pub fn nuisances(&self) -> Result<JsValue, JsValue> {
        let m = self.model()?;
        let out: Vec<Nuisance> = crate::impacts::nuisances(m)
            .into_iter()
            .map(|i| Nuisance { index: i, name: m.params[i].name.clone() })
            .collect();
        serde_wasm_bindgen::to_value(&out).map_err(js_err)
    }

    /// Impacts for a slice of nuisances, measured against `anchor`.
    ///
    /// Sliced rather than run whole because, unlike the scan, each nuisance is
    /// independent given the anchor — so a pool of workers can share the job.
    #[wasm_bindgen(js_name = impactsFor)]
    pub fn impacts_for(
        &self,
        anchor: JsValue,
        which: Vec<usize>,
        asymmetric: bool,
    ) -> Result<JsValue, JsValue> {
        let anchor: crate::Anchor = serde_wasm_bindgen::from_value(anchor).map_err(js_err)?;
        let cfg = FitConfig {
            poi_range: (self.cfg.r_min, self.cfg.r_max),
            minos: false,
            ..Default::default()
        };
        let out = crate::impacts_for(self.model()?, &cfg, &anchor, &which, asymmetric);
        serde_wasm_bindgen::to_value(&out).map_err(js_err)
    }

    /// Release the templates before the next card is loaded.
    #[wasm_bindgen(js_name = freeModel)]
    pub fn free_model(&mut self) {
        self.model = None;
    }
}

/// Parse a datacard and return its AST, without loading any templates.
#[wasm_bindgen(js_name = parseDatacard)]
pub fn parse_datacard(src: &str) -> Result<JsValue, JsValue> {
    let dc = Datacard::parse(src).map_err(js_err)?;
    serde_wasm_bindgen::to_value(&dc).map_err(js_err)
}
