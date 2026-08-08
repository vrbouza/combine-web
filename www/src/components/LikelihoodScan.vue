<template>
  <figure class="plot bg-white border border-gray-200 rounded-md p-4 mb-4">
    <figcaption class="flex items-center gap-2">
      <span>Profile likelihood — <span class="font-medium text-gray-700">{{ title }}</span></span>
      <Loader2 v-if="busy" class="w-3.5 h-3.5 animate-spin text-amber-500" />
      <span v-if="prefit" class="ml-auto text-xs text-gray-400">
        in units of the pre-fit width
      </span>
    </figcaption>
    <svg class="chart" :viewBox="`0 0 ${W} ${H}`" role="img" aria-label="Profile likelihood scan">
      <!-- 68% and 95% levels, at 1.0 and 3.84 for one degree of freedom -->
      <g v-for="lv in levels" :key="lv.y">
        <line :x1="P" :x2="W - P" :y1="y(lv.y)" :y2="y(lv.y)" class="level" />
        <text :x="W - P + 4" :y="y(lv.y) + 4" class="lvl">{{ lv.label }}</text>
      </g>
      <path :d="path" class="curve" />
      <line :x1="x(best)" :x2="x(best)" :y1="P" :y2="H - P" class="best" />
      <g v-for="c in marks" :key="c">
        <line :x1="x(c)" :x2="x(c)" :y1="y(1)" :y2="H - P" class="cross" />
      </g>
      <line :x1="P" :x2="W - P" :y1="H - P" :y2="H - P" class="axis" />
      <line :x1="P" :x2="P" :y1="P" :y2="H - P" class="axis" />
      <text v-for="t in xTicks" :key="t" :x="x(t)" :y="H - P + 18" class="tick">{{ fmt(t) }}</text>
      <text v-if="prefit" :x="W / 2" :y="H - 6" class="label">
        (θ̂ − θ<tspan class="sub">I</tspan>)/σ<tspan class="sub">I</tspan>
      </text>
      <text v-else :x="W / 2" :y="H - 6" class="label">{{ poi }}</text>
      <text :x="14" :y="H / 2" class="label" :transform="`rotate(-90 14 ${H / 2})`">
        2&#8710;lnL
      </text>
    </svg>
  </figure>
</template>

<script lang="ts">
import { computed, defineComponent, type PropType } from "vue";
import { Loader2 } from "@lucide/vue";
import type { ScanResult } from "@/types";

const W = 460;
const H = 320;
const P = 44;

export default defineComponent({
  components: { Loader2 },
  props: {
    scan: { type: Object as PropType<ScanResult>, required: true },
    poi: { type: String, default: "r" },
    /// Set when a nuisance is being profiled rather than the POI.
    paramName: { type: String, default: "" },
    /// `[centre - wLo, centre, centre + wHi]` from the prior. Present only for a
    /// nuisance, and it switches the axis into units of the pre-fit width.
    prefit: { type: Array as unknown as PropType<[number, number, number] | null>, default: null },
    busy: { type: Boolean, default: false },
  },
  setup(props) {
    // A nuisance is drawn in units of its own prior, so an unconstrained one is
    // its prior exactly — a parabola crossing 1 at +-1 — and a constrained one
    // is visibly narrower. That makes the curves comparable between rows.
    const norm = (v: number) => {
      const pre = props.prefit;
      if (!pre) return v;
      const d = v - pre[1];
      const w = d >= 0 ? pre[2] - pre[1] : pre[1] - pre[0];
      return w > 0 ? d / w : d;
    };

    const good = computed(() => props.scan.points.filter((p) => Number.isFinite(p.deltaNll)));
    const xs = computed(() => good.value.map((p) => norm(p.r)));
    // Clip the vertical range: a scan run over a wide window climbs far above
    // the region anyone reads off it.
    const yMax = computed(() => Math.min(Math.max(...good.value.map((p) => p.deltaNll), 4), 9));

    const x = (v: number) =>
      P + ((v - Math.min(...xs.value)) / (Math.max(...xs.value) - Math.min(...xs.value) || 1)) * (W - 2 * P);
    const y = (v: number) => H - P - (Math.min(v, yMax.value) / yMax.value) * (H - 2 * P);

    const path = computed(() =>
      good.value
        .map((p, i) => `${i ? "L" : "M"}${x(norm(p.r)).toFixed(2)} ${y(p.deltaNll).toFixed(2)}`)
        .join(" ")
    );
    const marks = computed(() =>
      [props.scan.crossings68?.[0], props.scan.crossings68?.[1]]
        .filter((v): v is number => v != null)
        .map(norm)
    );
    const xTicks = computed(() => {
      const [a, b] = [Math.min(...xs.value), Math.max(...xs.value)];
      return [0, 0.25, 0.5, 0.75, 1].map((f) => a + (b - a) * f);
    });

    return {
      W, H, P, x, y, path, marks, xTicks,
      best: computed(() => norm(props.scan.bestFit)),
      title: computed(() => props.paramName || props.poi),
      levels: computed(() =>
        [
          { y: 1, label: "68%" },
          { y: 3.84, label: "95%" },
        ].filter((l) => l.y <= yMax.value)
      ),
      fmt: (v: number) => (Math.abs(v) >= 100 ? v.toFixed(0) : v.toFixed(2)),
    };
  },
});
</script>

<style scoped>
.plot { margin: 0; }
figcaption { font-size: .8rem; color: #6b7280; margin-bottom: .25rem; }
/* Must not match the spinner icon in the caption: scoping turns a bare `svg`
   into `svg[data-v-…]`, which outranks Tailwind's own `w-3.5` sizing class. */
svg.chart { width: 100%; height: auto; background: #fff; border: 1px solid #e5e7eb; border-radius: .375rem; }
.curve { fill: none; stroke: #333; stroke-width: 1.8; }
.axis { stroke: #9ca3af; stroke-width: 1; }
.level { stroke: #d1d5db; stroke-dasharray: 4 3; }
.best { stroke: #c33; stroke-width: 1; stroke-dasharray: 3 3; }
.cross { stroke: #39c; stroke-width: 1; }
.tick, .lvl { font-size: 12px; fill: #9ca3af; text-anchor: middle; }
.lvl { text-anchor: start; }
.label { font-size: 14px; fill: #4b5563; text-anchor: middle; }
.sub { font-size: 10px; baseline-shift: sub; }
</style>
