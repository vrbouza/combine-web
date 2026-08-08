<template>
  <div class="flex flex-col gap-6 md:flex-row md:gap-0">
    <!-- Left rail: title, help, parameters -->
    <div class="w-full md:w-[350px] md:shrink-0">
      <h1 class="text-2xl font-medium mb-4 flex items-center gap-2">
        <ChartSpline class="w-6 h-6" />
        Maximum likelihood fit
      </h1>

      <FitHelpCollapsible />

      <TooltipProvider>
        <div class="flex flex-col gap-4">
          <div>
            <p class="flex items-center gap-1">
              <Tooltip>
                <TooltipTrigger as-child>
                  <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
                </TooltipTrigger>
                <TooltipContent>
                  <p class="max-w-xs">
                    Range the signal strength may take, as Combine's --rMin/--rMax.
                  </p>
                </TooltipContent>
              </Tooltip>
              Signal strength range
            </p>
            <div class="flex flex-row items-center w-full gap-2">
              <input v-model.number="cfg.rMin" type="number" step="any" :disabled="running"
                     class="w-full border border-gray-300 rounded-md px-2 py-1 text-sm" />
              <span class="text-gray-400 text-sm">to</span>
              <input v-model.number="cfg.rMax" type="number" step="any" :disabled="running"
                     class="w-full border border-gray-300 rounded-md px-2 py-1 text-sm" />
            </div>
          </div>

          <div>
            <p class="flex items-center gap-1">
              <Tooltip>
                <TooltipTrigger as-child>
                  <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
                </TooltipTrigger>
                <TooltipContent>
                  <p class="max-w-xs">
                    Substituted for $MASS in the card's shapes patterns, so templates such as
                    ggH200 resolve.
                  </p>
                </TooltipContent>
              </Tooltip>
              Mass
            </p>
            <input v-model="cfg.mass" type="text" :disabled="running"
                   class="w-full border border-gray-300 rounded-md px-2 py-1 text-sm" />
          </div>

          <div>
            <p class="flex items-center gap-1">
              <Tooltip>
                <TooltipTrigger as-child>
                  <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
                </TooltipTrigger>
                <TooltipContent>
                  <p class="max-w-xs">
                    Each point is a full minimisation with the parameter of interest fixed, so this
                    dominates the running time. Zero skips the scan.
                  </p>
                </TooltipContent>
              </Tooltip>
              Scan points
            </p>
            <div class="flex flex-row items-center w-full gap-2">
              <input v-model.number="cfg.scanPoints" type="range" min="0" max="200" step="10"
                     :disabled="running" class="flex-grow" />
              <span class="block w-[40px] text-center border border-gray-300 rounded-md text-sm">
                {{ cfg.scanPoints }}
              </span>
            </div>
          </div>

          <div class="flex flex-row items-center w-full gap-2">
            <input id="minos" type="checkbox" v-model="cfg.minos" :disabled="running" />
            <Tooltip>
              <TooltipTrigger as-child>
                <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p class="max-w-xs">
                  Asymmetric errors from the profile likelihood rather than the parabolic HESSE
                  approximation.
                </p>
              </TooltipContent>
            </Tooltip>
            <label for="minos">MINOS errors</label>
          </div>

          <div class="flex flex-row items-center w-full gap-2">
            <input id="ignore" type="checkbox" v-model="cfg.ignoreShapes" :disabled="running" />
            <Tooltip>
              <TooltipTrigger as-child>
                <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p class="max-w-xs">
                  Fits normalisation systematics only. Much faster, but less constrained than
                  Combine, so the interval is narrower and not comparable.
                </p>
              </TooltipContent>
            </Tooltip>
            <label for="ignore">Ignore shape systematics</label>
          </div>

          <div class="flex flex-row items-center w-full gap-2">
            <input id="impacts" type="checkbox" v-model="cfg.computeImpacts" :disabled="running" />
            <Tooltip>
              <TooltipTrigger as-child>
                <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p class="max-w-xs">
                  Ranks the nuisances by how much each one moves the POI, and lets you click a row
                  to profile that parameter. Costs two extra fits per nuisance, spread across the
                  worker pool below.
                </p>
              </TooltipContent>
            </Tooltip>
            <label for="impacts">Compute impacts</label>
          </div>

          <div v-if="cfg.computeImpacts" class="flex flex-row items-center w-full gap-2 pl-5">
            <input id="asym" type="checkbox" v-model="cfg.impactsAsymmetric" :disabled="running" />
            <Tooltip>
              <TooltipTrigger as-child>
                <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
              </TooltipTrigger>
              <TooltipContent>
                <p class="max-w-xs">
                  Displaces each nuisance by its own MINOS crossings, as Combine does by default.
                  Turn off for the symmetric HESSE error, which is faster and is what Combine
                  itself falls back to when a crossing search fails.
                </p>
              </TooltipContent>
            </Tooltip>
            <label for="asym" class="text-sm">Asymmetric errors</label>
          </div>

          <div v-if="cfg.computeImpacts" class="pl-5">
            <p class="flex items-center gap-1 text-sm">
              <Tooltip>
                <TooltipTrigger as-child>
                  <Info class="w-3.5 h-3.5 text-gray-400 cursor-help" />
                </TooltipTrigger>
                <TooltipContent>
                  <p class="max-w-xs">
                    How many web workers share the impact fits. Each holds its own copy of the
                    model, so more threads cost memory as well as cores, and the returns fall off
                    well before the maximum.
                  </p>
                </TooltipContent>
              </Tooltip>
              Threads
            </p>
            <div class="flex flex-row items-center w-full gap-2">
              <input v-model.number="cfg.impactWorkers" type="range" min="1" :max="maxWorkers"
                     :disabled="running" class="flex-grow accent-gray-700" />
              <span class="block w-[40px] text-center border border-gray-300 rounded-md text-sm">
                {{ cfg.impactWorkers }}
              </span>
            </div>
          </div>
        </div>
      </TooltipProvider>
    </div>

    <!-- Right column: input, status, results -->
    <div class="min-w-0 w-full flex-1 overflow-visible pt-0 md:overflow-hidden md:pt-12">
      <div v-if="error"
           class="mx-6 mb-4 p-3 bg-red-50 border border-red-300 rounded-md text-sm text-red-800">
        <p class="font-medium">{{ errorTitle }}</p>
        <p class="mt-1 font-mono text-xs break-words">{{ error.detail }}</p>
      </div>

      <div v-for="w in warnings" :key="w"
           class="mx-6 mb-4 p-3 bg-amber-50 border border-amber-400 rounded-md text-sm text-gray-700">
        {{ w }}
      </div>

      <div v-if="summary && summary.ignoredShapeSysts.length"
           class="mx-6 mb-4 p-3 bg-amber-50 border border-amber-400 rounded-md text-sm text-gray-700">
        {{ summary.ignoredShapeSysts.length }} shape systematics ignored, so this interval is
        narrower than Combine's and not comparable to it.
      </div>

      <div v-bind="getRootProps()"
           :class="[
             'p-6 mx-6 bg-white border rounded-md flex flex-col justify-center items-center gap-2 text-gray-600',
             'cursor-pointer',
             isDragActive ? 'border-gray-400' : 'border-gray-200 hover:border-gray-400',
           ]">
        <input v-bind="getInputProps()" />
        <FileUp />
        <p v-if="isDragActive">Drop them here ...</p>
        <p v-else>Drop or click to upload your <b>datacard</b> and any <b>.root</b> files</p>
      </div>

      <div v-if="card || shapes.length" class="mx-6 mt-4">
        <div v-if="card" class="flex items-center gap-2 py-2 px-3 bg-gray-50 rounded-md mb-2">
          <Check class="w-4 h-4 text-green-500" />
          <span class="flex-grow text-sm font-mono truncate">{{ card.name }}</span>
          <span class="text-xs text-gray-400">datacard</span>
        </div>
        <div v-for="f in shapes" :key="f.name"
             class="flex items-center gap-2 py-2 px-3 bg-gray-50 rounded-md mb-2">
          <Check class="w-4 h-4 text-green-500" />
          <span class="flex-grow text-sm font-mono truncate">{{ f.name }}</span>
          <span class="text-xs text-gray-400">shapes</span>
        </div>
      </div>

      <div class="mx-6 mt-4 flex flex-wrap items-center gap-2">
        <Button variant="outline" size="sm" :disabled="!card || running" @click="run">
          <Loader2 v-if="running" class="mr-1 h-3 w-3 animate-spin text-amber-500" />
          <Play v-else class="mr-1 h-3 w-3" />
          {{ running ? stage || "Working…" : "Run fit" }}
        </Button>
        <Button v-if="card && !running" variant="outline" size="sm" @click="clear">
          <Trash2 class="mr-1 h-3 w-3" /> Clear
        </Button>

        <!-- Exports share the action bar rather than sitting below the results:
             those run to several screens now, and the download is the first
             thing wanted once the fit has been read. -->
        <Button v-if="result" variant="outline" size="sm" class="ml-auto" @click="download">
          <Download class="mr-1 h-3 w-3" /> Download results (.json)
        </Button>
        <Button v-if="result && impacts && impacts.length" variant="outline" size="sm"
                @click="downloadImpacts">
          <Download class="mr-1 h-3 w-3" /> Download impacts.json (Combine format)
        </Button>
      </div>

      <div v-if="result" class="mx-6 mt-6">
        <div class="bg-white border border-gray-200 rounded-md p-4 mb-4">
          <p class="text-lg">
            <span class="font-medium">{{ result.poi.name }}</span>
            =
            <span class="tabular-nums">{{ result.poi.value.toFixed(6) }}</span>
            <template v-if="result.minos">
              <span class="text-gray-600 ml-1 tabular-nums">
                {{ fmt(result.minos[0]) }} / {{ fmt(result.minos[1]) }}
              </span>
              <span class="text-gray-400 text-sm ml-1">(68% CL, MINOS)</span>
            </template>
            <template v-else>
              <span class="text-gray-600 ml-1 tabular-nums">
                ± {{ result.poi.error.toFixed(6) }}
              </span>
              <span class="text-gray-400 text-sm ml-1">(HESSE)</span>
            </template>
          </p>
          <p v-if="scan && scan.crossings68" class="text-sm text-gray-500 mt-1 tabular-nums">
            scan 68%: {{ scan.crossings68[0].toFixed(6) }} .. {{ scan.crossings68[1].toFixed(6) }}
            <template v-if="scan.crossings95">
              · 95%: {{ scan.crossings95[0].toFixed(6) }} .. {{ scan.crossings95[1].toFixed(6) }}
            </template>
          </p>
          <p class="text-xs text-gray-400 mt-2">
            edm {{ result.edm.toExponential(2) }} · {{ result.nfcn }} calls · cascade step
            {{ result.cascadeStep }} · {{ elapsedMs.toFixed(0) }} ms
            <template v-if="summary">
              · {{ summary.nBins }} bins, {{ summary.nParams }} parameters
            </template>
          </p>
        </div>

        <div v-if="impactsProgress"
             class="bg-white border border-gray-200 rounded-md p-4 mb-4 flex items-center gap-2">
          <Loader2 class="w-4 h-4 animate-spin text-amber-500" />
          <span class="text-sm text-gray-600">
            Impacts {{ impactsProgress.done }}/{{ impactsProgress.total }} across
            {{ impactsProgress.workers }} worker{{ impactsProgress.workers === 1 ? "" : "s" }}
          </span>
        </div>
        <div v-if="impactsError"
             class="mb-4 p-3 bg-red-50 border border-red-300 rounded-md text-sm text-red-800">
          <p class="font-medium">The impacts could not be computed</p>
          <p class="mt-1 font-mono text-xs break-words">{{ impactsError }}</p>
        </div>

        <!-- The profile and the ranking are what a reader compares, so they
             share a row; `min-w-0` is what lets these columns actually shrink. -->
        <div class="flex flex-col gap-4 2xl:flex-row 2xl:items-start mb-4">
          <div class="min-w-0 w-full 2xl:w-1/3">
            <LikelihoodScan
              v-if="shownScan"
              :scan="shownScan" :poi="result.poi.name"
              :param-name="selectedParam ? selectedParam.name : ''"
              :prefit="selectedPrefit" :busy="paramScanRunning"
            />
          </div>
          <div class="min-w-0 w-full 2xl:flex-1">
            <ImpactsPlot
              v-if="impacts && impacts.length"
              :impacts="impacts" :poi="result.poi.name" :poi-fit="poiFit"
              :selected="selectedParam ? selectedParam.name : null"
              @select="pick"
            />
          </div>
        </div>

        <div class="flex flex-col gap-4 2xl:flex-row 2xl:items-start">
          <div class="min-w-0 w-full 2xl:w-1/3">
            <CorrelationMatrix
              v-if="report && report.correlation.length"
              :names="report.names"
              :correlation="report.correlation"
            />
          </div>
          <div class="min-w-0 w-full 2xl:flex-1">
            <!-- Every fitted parameter, so on a real card this is hundreds of
                 rows: bounded, with the header pinned. -->
            <div class="overflow-auto max-h-[560px] border border-gray-200 rounded-md bg-white">
              <table class="w-full text-sm">
                <thead>
                  <tr class="bg-gray-50 text-left sticky top-0">
                    <th class="px-3 py-2 font-medium text-gray-700">Parameter</th>
                    <th class="px-3 py-2 font-medium text-gray-700">Value</th>
                    <th class="px-3 py-2 font-medium text-gray-700">Error</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="p in result.params" :key="p.name" class="border-t border-gray-100">
                    <td class="px-3 py-2 font-mono truncate max-w-[320px]">{{ p.name }}</td>
                    <td class="px-3 py-2 tabular-nums">{{ p.value.toExponential(4) }}</td>
                    <td class="px-3 py-2 tabular-nums">{{ p.error.toExponential(2) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>

      </div>
    </div>
  </div>
</template>

<script lang="ts">
import { computed, defineComponent, reactive, ref } from "vue";
import { useStore } from "vuex";
import { useDropzone } from "vue3-dropzone";
import {
  ChartSpline, Check, Download, FileUp, Info, Loader2, Play, Trash2,
} from "@lucide/vue";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import FitHelpCollapsible from "@/components/help/FitHelpCollapsible.vue";
import LikelihoodScan from "@/components/LikelihoodScan.vue";
import ImpactsPlot from "@/components/ImpactsPlot.vue";
import CorrelationMatrix from "@/components/CorrelationMatrix.vue";
import { defaultConfig } from "@/types";
import type { RootState } from "@/types";

export default defineComponent({
  components: {
    ChartSpline, Check, Download, FileUp, Info, Loader2, Play, Trash2,
    Button, Tooltip, TooltipContent, TooltipProvider, TooltipTrigger,
    FitHelpCollapsible, LikelihoodScan, ImpactsPlot, CorrelationMatrix,
  },
  setup() {
    const store = useStore<RootState>();
    const card = ref<File | null>(null);
    const shapes = ref<File[]>([]);
    const cfg = reactive(defaultConfig());

    // One zone for both kinds of input: `accept` is omitted so nothing is ever
    // rejected, and we classify by extension ourselves.
    const onDrop = (accepted: File[]) => {
      for (const f of accepted) {
        if (f.name.toLowerCase().endsWith(".root")) {
          // onDrop fires per drop rather than cumulatively, so merge: the card
          // and its shape files may arrive in separate gestures.
          shapes.value = [...shapes.value.filter((s) => s.name !== f.name), f];
        } else {
          card.value = f;
        }
      }
    };
    // Only `isDragActive` is usable: mid-drag the browser exposes no filenames,
    // so `isDragReject` would be true for everything.
    const { getRootProps, getInputProps, isDragActive } = useDropzone({ onDrop });

    const clear = () => {
      card.value = null;
      shapes.value = [];
    };
    const run = () => {
      if (card.value) {
        store.dispatch("fit", { card: card.value, shapes: shapes.value, cfg: { ...cfg } });
      }
    };

    const result = computed(() => store.state.result);
    const summary = computed(() => store.state.summary);
    const scan = computed(() => store.state.scan);
    const report = computed(() => store.state.report);
    const error = computed(() => store.state.error);
    const impacts = computed(() => store.state.impacts);
    const selectedParam = computed(() => store.state.selectedParam);

    /// Clicking a row profiles that parameter; clicking the r-hat readout, or
    /// the same row again, goes back to the POI.
    const pick = (name: string | null) => {
      const cur = store.state.selectedParam;
      const next =
        name === null || cur?.name === name
          ? null
          : store.state.nuisances.find((n) => n.name === name) ?? null;
      store.dispatch("selectParam", { param: next, cfg: { ...cfg } });
    };

    /// The POI's scan comes with the fit; a nuisance's is fetched on click and
    /// cached, so this is null only while the first one is still running.
    const shownScan = computed(() => {
      const sel = store.state.selectedParam;
      return sel ? store.state.paramScans[sel.name] ?? null : store.state.scan;
    });

    /// Present only for a nuisance, and it switches the scan's axis into units
    /// of the pre-fit width. Every `Impact` already carries the triplet.
    const selectedPrefit = computed(() => {
      const sel = store.state.selectedParam;
      if (!sel) return null;
      return store.state.impacts?.find((i) => i.name === sel.name)?.prefit ?? null;
    });

    /// `[best + lo, best, best + hi]`, the triplet Combine writes for the POI
    /// and the impacts plot prints in its header.
    const poiFit = computed((): [number, number, number] => {
      const r = result.value;
      if (!r) return [0, 0, 0];
      const [lo, hi] = r.minos ?? [-r.poi.error, r.poi.error];
      return [r.poi.value + lo, r.poi.value, r.poi.value + hi];
    });

    const errorTitle = computed(() => {
      switch (error.value?.kind) {
        case "parse": return "The datacard could not be used";
        case "shapes": return "A shape file was missing or unreadable";
        case "converge": return "The fit did not converge";
        default: return "Something went wrong";
      }
    });

    const save = (doc: unknown, name: string) => {
      const blob = new Blob([JSON.stringify(doc, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = name;
      a.click();
      // Revoking immediately can cancel the download in some browsers.
      setTimeout(() => URL.revokeObjectURL(url), 0);
    };

    const download = () =>
      save(
        { summary: summary.value, result: result.value, report: report.value,
          scan: scan.value, impacts: impacts.value, warnings: store.state.warnings },
        "kestrel_fit.json"
      );

    /// Combine's own `impacts.json` layout, so `plotImpacts.py` renders these
    /// numbers without modification.
    const downloadImpacts = () => {
      const name = result.value?.poi.name ?? "r";
      save(
        {
          POIs: [{ name, fit: poiFit.value }],
          method: "default",
          params: (impacts.value ?? []).map((p) => ({
            name: p.name, type: p.kind, groups: [],
            prefit: p.prefit, fit: p.fit, [name]: p.poi,
            [`impact_${name}`]: p.impact, converged: p.ok,
          })),
        },
        "impacts.json"
      );
    };

    return {
      card, shapes, cfg, run, download, downloadImpacts, clear,
      getRootProps, getInputProps, isDragActive,
      result, summary, error, errorTitle, scan, report, impacts, poiFit,
      selectedParam, selectedPrefit, shownScan, pick,
      paramScanRunning: computed(() => store.state.paramScanRunning),
      maxWorkers: Math.max(1, Math.min(navigator.hardwareConcurrency || 4, 16)),
      warnings: computed(() => store.state.warnings),
      stage: computed(() => store.state.stage),
      running: computed(() => store.state.running),
      elapsedMs: computed(() => store.state.elapsedMs),
      impactsProgress: computed(() => store.state.impactsProgress),
      impactsError: computed(() => store.state.impactsError),
      cores: Math.min(navigator.hardwareConcurrency || 4, 8),
      fmt: (v: number) => (v >= 0 ? "+" : "") + v.toFixed(6),
    };
  },
});
</script>
