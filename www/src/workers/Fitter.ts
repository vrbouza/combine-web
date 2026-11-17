import type { Anchor, Config } from "@/types";

/// Owns the wasm module and runs fits inside the worker, so the main thread
/// never blocks on a minimisation.
export class Fitter {
  private worker: Worker;
  private wasm: typeof import("@/pkg_kestrel") | null = null;
  private wasmPromise: Promise<typeof import("@/pkg_kestrel")>;
  /// Kept alive only in impacts mode, where one model serves many requests.
  private held: InstanceType<typeof import("@/pkg_kestrel").WasmFitter> | null = null;

  constructor(worker: Worker) {
    this.worker = worker;
    this.wasmPromise = import("@/pkg_kestrel").then((w) => {
      this.wasm = w;
      return w;
    });
  }

  private async load() {
    return this.wasm ?? (await this.wasmPromise);
  }

  async run(card: string, files: Map<string, Uint8Array>, cfg: Config): Promise<void> {
    let wasm;
    try {
      wasm = await this.load();
    } catch (e) {
      // A download failure must not be reported as a wasm panic.
      this.post("generic", e);
      return;
    }

    let fitter: InstanceType<typeof wasm.WasmFitter> | undefined;
    const started = performance.now();
    try {
      this.stage("Building the model");
      // The previous card's templates go back now, not at the end of this run:
      // the model is kept alive afterwards so a profile scan can use it.
      this.dispose();
      fitter = new wasm.WasmFitter(card, new Map(files) as never, cfg);
    } catch (e) {
      this.post("parse", e);
      return;
    }

    try {
      const summary = fitter.summary();
      const warnings = fitter.warnings();

      this.stage("Fitting");
      const result = fitter.fit();
      const report = fitter.report();

      // The scan is many minimisations, so it is reported separately: the fit
      // result is already worth showing by the time it starts.
      let scan = null;
      if (cfg.scanPoints > 0) {
        this.stage(`Scanning ${cfg.scanPoints} points`);
        scan = fitter.scan(cfg.scanPoints);
      }

      this.worker.postMessage({
        done: true,
        summary,
        result,
        report,
        scan,
        warnings,
        // Sent with the result so an impacts run needs no extra round trip.
        nuisances: fitter.nuisances(),
        elapsedMs: performance.now() - started,
      });
      // Kept rather than freed: clicking a row in the impacts plot profiles
      // that parameter, and rebuilding the model for each click would be far
      // more expensive than holding it.
      this.held = fitter;
      fitter = undefined;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      this.post(/converge/i.test(msg) ? "converge" : "generic", e);
    } finally {
      fitter?.free();
    }
  }

  /// Profile likelihood of one parameter, on demand when a row is clicked.
  scanParam(index: number, points: number): void {
    if (!this.held) {
      this.post("generic", new Error("No model is loaded. Run a fit first"));
      return;
    }
    try {
      this.worker.postMessage({ paramScan: this.held.scanParam(index, points), index });
    } catch (e) {
      this.worker.postMessage({
        paramScanFailed: true,
        index,
        detail: e instanceof Error ? e.message : String(e),
      });
    }
  }

  /// Build a model and keep it, so this worker can serve many impact requests.
  /// One member of a pool; each holds its own wasm instance, which is why no
  /// SharedArrayBuffer and no cross-origin isolation headers are needed.
  async init(card: string, files: Map<string, Uint8Array>, cfg: Config): Promise<void> {
    try {
      const wasm = await this.load();
      this.held?.free();
      this.held = new wasm.WasmFitter(card, new Map(files) as never, cfg);
      this.worker.postMessage({ ready: true });
    } catch (e) {
      this.post("parse", e);
    }
  }

  /// Impacts for one slice of nuisances. `which` comes back untouched so the
  /// pool can tell which request a reply answers.
  impacts(anchor: Anchor, which: number[], asymmetric: boolean): void {
    if (!this.held) {
      this.post("generic", new Error("Impacts requested before the model was built"));
      return;
    }
    try {
      const impacts = this.held.impactsFor(anchor, new Uint32Array(which), asymmetric);
      this.worker.postMessage({ impacts, which });
    } catch (e) {
      // One bad slice must not kill the run: the pool records it and carries on.
      this.worker.postMessage({
        impacts: [],
        which,
        detail: e instanceof Error ? e.message : String(e),
      });
    }
  }

  dispose(): void {
    this.held?.free();
    this.held = null;
  }

  private stage(stage: string): void {
    this.worker.postMessage({ stage });
  }

  private post(kind: string, e: unknown): void {
    this.worker.postMessage({
      error: true,
      kind,
      detail: e instanceof Error ? e.message : String(e),
    });
  }
}
