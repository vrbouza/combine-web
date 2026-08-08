import { createStore } from "vuex";
import { ImpactsPool, type PoolProgress } from "@/workers/ImpactsPool";
import type {
  Config, ErrorKind, FitResult, Impact, Nuisance, Report, RootState, ScanResult, Summary,
} from "@/types";

/// Held outside the state: a live pool is not reactive data, and Vuex would
/// otherwise try to make the workers observable.
let pool: ImpactsPool | null = null;

/// The store owns all worker traffic; a page only ever dispatches.
export default createStore<RootState>({
  state: {
    worker: null,
    running: false,
    summary: null,
    result: null,
    scan: null,
    report: null,
    warnings: [],
    stage: "",
    error: null,
    elapsedMs: 0,
    impacts: null,
    impactsProgress: null,
    impactsError: null,
    nuisances: [],
    selectedParam: null,
    paramScans: {},
    paramScanRunning: false,
  },
  mutations: {
    SET_WORKER(state, worker: Worker) {
      state.worker = worker;
    },
    START(state) {
      state.running = true;
      state.error = null;
      state.result = null;
      state.summary = null;
      state.scan = null;
      state.report = null;
      state.warnings = [];
      state.stage = "Starting";
      state.impacts = null;
      state.impactsProgress = null;
      state.impactsError = null;
      state.nuisances = [];
      state.selectedParam = null;
      state.paramScans = {};
      state.paramScanRunning = false;
      pool?.terminate();
      pool = null;
    },
    STAGE(state, stage: string) {
      state.stage = stage;
    },
    DONE(
      state,
      p: {
        summary: Summary;
        result: FitResult;
        report: Report;
        scan: ScanResult | null;
        warnings: string[];
        nuisances: Nuisance[];
        elapsedMs: number;
      }
    ) {
      state.running = false;
      state.stage = "";
      state.summary = p.summary;
      state.result = p.result;
      state.report = p.report;
      state.scan = p.scan;
      state.warnings = p.warnings ?? [];
      state.nuisances = p.nuisances ?? [];
      state.elapsedMs = p.elapsedMs;
    },
    FAIL(state, p: { kind: ErrorKind; detail: string }) {
      state.running = false;
      state.stage = "";
      state.error = p;
    },
    /// Null selects the POI, whose scan is already computed.
    SELECT_PARAM(state, p: Nuisance | null) {
      state.selectedParam = p;
    },
    PARAM_SCAN_START(state) {
      state.paramScanRunning = true;
    },
    PARAM_SCAN_DONE(state, p: { name: string; scan: ScanResult }) {
      state.paramScans = { ...state.paramScans, [p.name]: p.scan };
      state.paramScanRunning = false;
    },
    /// A profile that will not converge falls back to the POI rather than
    /// leaving the plot showing someone else's curve under the wrong title.
    PARAM_SCAN_FAIL(state, detail: string) {
      state.paramScanRunning = false;
      state.selectedParam = null;
      state.impactsError = detail;
    },
    IMPACTS_PROGRESS(state, p: PoolProgress) {
      state.impactsProgress = p;
    },
    IMPACTS_DONE(state, impacts: Impact[]) {
      state.impacts = impacts;
      state.impactsProgress = null;
      state.running = false;
      state.stage = "";
    },
    /// Impacts are a diagnostic on top of a fit that already succeeded, so a
    /// failure here leaves the fit on screen and reports itself separately.
    IMPACTS_FAIL(state, detail: string) {
      state.impactsError = detail;
      state.impactsProgress = null;
      state.running = false;
      state.stage = "";
    },
  },
  actions: {
    async fit({ state, commit, dispatch }, p: { card: File; shapes: File[]; cfg: Config }) {
      if (!state.worker) return;
      commit("START");

      const card = await p.card.text();
      const files: Array<[string, Uint8Array]> = [];
      for (const f of p.shapes) {
        files.push([f.name, new Uint8Array(await f.arrayBuffer())]);
      }

      state.worker.onmessage = (msg) => {
        if (msg.data.error) {
          commit("FAIL", { kind: msg.data.kind, detail: msg.data.detail });
        } else if (msg.data.stage) {
          commit("STAGE", msg.data.stage);
        } else if (msg.data.done) {
          commit("DONE", msg.data);
          // Impacts need the fit's own best-fit values as their anchor, so they
          // can only start once it has returned.
          if (p.cfg.computeImpacts) {
            void dispatch("impacts", { card, files, cfg: p.cfg, nuisances: msg.data.nuisances });
          }
        }
      };
      state.worker.postMessage({ fit: true, card, files, cfg: p.cfg });
    },

    /// Profile one parameter. `param` null selects the POI, whose scan came
    /// with the fit; anything else is computed on demand and then cached, since
    /// each costs a run of minimisations and users click back and forth.
    selectParam({ state, commit }, p: { param: Nuisance | null; cfg: Config }) {
      commit("SELECT_PARAM", p.param);
      if (!p.param || !state.worker) return;
      if (state.paramScans[p.param.name]) return;

      commit("PARAM_SCAN_START");
      const points = Math.max(12, Math.round(p.cfg.scanPoints / 2));
      const name = p.param.name;
      state.worker.onmessage = (msg) => {
        if (msg.data.paramScan) {
          commit("PARAM_SCAN_DONE", { name, scan: msg.data.paramScan });
        } else if (msg.data.paramScanFailed) {
          commit("PARAM_SCAN_FAIL", msg.data.detail);
        } else if (msg.data.error) {
          commit("PARAM_SCAN_FAIL", msg.data.detail);
        }
      };
      state.worker.postMessage({ scanParam: true, index: p.param.index, points });
    },

    async impacts(
      { state, commit },
      p: {
        card: string;
        files: Array<[string, Uint8Array]>;
        cfg: Config;
        nuisances: Nuisance[];
      }
    ) {
      const res = state.result;
      if (!res || !p.nuisances?.length) return;

      state.running = true;
      commit("STAGE", "Computing impacts");
      // The anchor is just the fit result reshaped, so no extra minimisation is
      // needed before the pool can start.
      const anchor = {
        best: res.params.map((q) => q.value),
        error: res.params.map((q) => q.error),
        poi: res.poi.value,
      };

      pool = new ImpactsPool();
      try {
        const out = await pool.run(p.card, p.files, p.cfg, anchor, p.nuisances, (prog) =>
          commit("IMPACTS_PROGRESS", prog)
        );
        commit("IMPACTS_DONE", out);
      } catch (e) {
        commit("IMPACTS_FAIL", e instanceof Error ? e.message : String(e));
      } finally {
        pool = null;
      }
    },
  },
});
