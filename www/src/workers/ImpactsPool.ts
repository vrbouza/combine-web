import WorkerFitter from "@/workers/Fitter.worker";
import type { Anchor, Config, Impact, Nuisance } from "@/types";

/// Each request re-does one minimisation to seed its crossing searches, so a
/// worker wants few large requests; work stealing wants many small ones. A few
/// per worker splits the difference. Measured on a 72-nuisance card, this is
/// worth about 10% against a fixed chunk of two, all of it single-worker: the
/// parallel wall clock is set by the cores, not by the chunking.
const CHUNKS_PER_WORKER = 3;

/// A hard ceiling on the slider, not a target. Each worker holds its own model
/// and its own wasm instance, so memory scales with the pool and every one of
/// them compiles the module again. Returns fall off well before this: 72
/// nuisances took 5.3 s on one worker, 3.3 on two, 2.8 on four and 1.9 on
/// eight, since a laptop's "12 cores" are mostly efficiency cores.
const MAX_WORKERS = 16;

export interface PoolProgress {
  done: number;
  total: number;
  workers: number;
}

/// Runs impacts across a pool of workers.
///
/// Unlike the scan, whose points are warm started from their neighbours and so
/// must run in sequence, every nuisance is independent once the anchor is
/// known. That makes this embarrassingly parallel, and it is the one place in
/// the app where more cores are worth the complexity.
export class ImpactsPool {
  private workers: Worker[] = [];
  private cancelled = false;

  /// Deliberately plain `new Worker()` instances rather than wasm threads:
  /// shared-memory threading would need SharedArrayBuffer, and so COOP/COEP
  /// cross-origin isolation headers, which break plain static hosting.
  async run(
    card: string,
    files: Array<[string, Uint8Array]>,
    cfg: Config,
    anchor: Anchor,
    nuisances: Nuisance[],
    onProgress: (p: PoolProgress) => void
  ): Promise<Impact[]> {
    const size = Math.max(1, Math.min(cfg.impactWorkers || 8, MAX_WORKERS, nuisances.length));
    const chunkSize = Math.max(1, Math.ceil(nuisances.length / (size * CHUNKS_PER_WORKER)));

    const queue = nuisances.map((n) => n.index);
    const out: Impact[] = [];
    const failures: string[] = [];
    let done = 0;

    onProgress({ done: 0, total: nuisances.length, workers: size });

    try {
      const runs = Array.from({ length: size }, () => {
        const w = new WorkerFitter();
        this.workers.push(w);
        return new Promise<void>((resolve, reject) => {
          const next = () => {
            const chunk = queue.splice(0, chunkSize);
            if (!chunk.length || this.cancelled) {
              resolve();
              return;
            }
            w.postMessage({
              impactsRun: true,
              anchor,
              which: chunk,
              asymmetric: cfg.impactsAsymmetric,
            });
          };

          w.onerror = (e) => reject(new Error(e.message || "an impacts worker crashed"));
          w.onmessage = (msg) => {
            const d = msg.data;
            if (d.error) {
              reject(new Error(d.detail));
            } else if (d.ready) {
              next();
            } else if (d.impacts) {
              out.push(...d.impacts);
              if (d.detail) failures.push(d.detail);
              done += d.which.length;
              onProgress({ done, total: nuisances.length, workers: size });
              next();
            }
          };

          // The `.root` bytes are structured-cloned into each worker rather
          // than transferred: a transfer is a move, so it could serve only one.
          w.postMessage({ impactsInit: true, card, files, cfg });
        });
      });

      await Promise.all(runs);
    } finally {
      this.terminate();
    }

    if (!out.length && failures.length) throw new Error(failures[0]);
    // Combine's own ordering: largest absolute shift first.
    out.sort((a, b) => b.impact - a.impact);
    return out;
  }

  terminate(): void {
    this.cancelled = true;
    for (const w of this.workers) {
      w.postMessage({ dispose: true });
      w.terminate();
    }
    this.workers = [];
  }
}
