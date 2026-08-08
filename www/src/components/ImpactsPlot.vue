<template>
  <figure class="plot bg-white border border-gray-200 rounded-md p-4 mb-4">
    <figcaption class="flex items-center justify-between gap-4">
      <span>
        Nuisance pulls and impacts. The impact is the shift in {{ poi }} when a nuisance is
        fixed at its post-fit &plusmn;1&sigma; and everything else is re-profiled &mdash; a
        measure of correlation, not a decomposition of the uncertainty.
      </span>
      <span class="shrink-0 flex gap-1">
        <button
          v-for="s in sorts" :key="s"
          class="sort" :class="{ on: sort === s }"
          @click="sort = s"
        >{{ s }}</button>
      </span>
    </figcaption>

    <svg :viewBox="`0 0 ${W} ${height}`" role="img" aria-label="Nuisance impacts">
      <!-- Alternating bands, as Combine shades them, so long names stay on their row -->
      <rect
        v-for="(r, i) in rows" :key="`band-${r.name}`"
        v-show="i % 2 === 0"
        :x="0" :y="top(i)" :width="W" :height="ROW" class="band"
      />

      <g v-for="(r, i) in rows" :key="`row-${r.name}`" class="pick"
         @click="$emit('select', r.name)">
        <!-- The hit area is the whole row, not just the text. -->
        <rect :x="0" :y="top(i)" :width="W" :height="ROW"
              :class="selected === r.name ? 'sel' : ''" fill-opacity="0.65"
              :fill="selected === r.name ? '#dbeafe' : 'transparent'" />
        <text :x="RANK" :y="mid(i) + 3.5" class="rank">{{ i + 1 }}</text>
        <text :x="NAME" :y="mid(i) + 3.5" class="name" :fill="colour(r.kind)">
          {{ r.ok ? r.name : `${r.name} †` }}
        </text>

        <!-- Unconstrained parameters have no prior to scale against, so Combine
             prints the absolute value where the pull would have gone. -->
        <template v-if="r.unconstrained">
          <rect :x="PULL0" :y="top(i) + 1" :width="PULL1 - PULL0" :height="ROW - 2" class="mask" />
          <text :x="(PULL0 + PULL1) / 2" :y="mid(i) + 3.5" class="absolute">
            {{ r.fit[1].toFixed(3) }} −{{ (r.fit[1] - r.fit[0]).toFixed(3) }} /
            +{{ (r.fit[2] - r.fit[1]).toFixed(3) }}
          </text>
        </template>
        <template v-else>
          <line
            :x1="xPull(r.scFit - r.scLo)" :x2="xPull(r.scFit + r.scHi)"
            :y1="fitY(i)" :y2="fitY(i)" class="bar"
          />
          <line v-for="e in [r.scFit - r.scLo, r.scFit + r.scHi]" :key="e"
                :x1="xPull(e)" :x2="xPull(e)" :y1="fitY(i) - 3" :y2="fitY(i) + 3" class="bar" />
          <circle :cx="xPull(r.scFit)" :cy="fitY(i)" r="2.6" class="dot" />
          <path v-if="r.pull !== null" :d="cross(xPull(r.pull), pullY(i))" class="pull" />
        </template>

        <!-- +1s first and -1s over it, both anchored at zero and both signed:
             two independent re-fits often shift the POI the same way. -->
        <rect
          :x="Math.min(xImp(0), xImp(r.impHi))" :y="top(i)"
          :width="Math.abs(xImp(r.impHi) - xImp(0))" :height="ROW" class="hi"
        />
        <rect
          :x="Math.min(xImp(0), xImp(r.impLo))" :y="top(i)"
          :width="Math.abs(xImp(r.impLo) - xImp(0))" :height="ROW" class="lo"
        />
        <title>{{ r.tip }}</title>
      </g>

      <line v-for="t in [-2, -1, 0, 1, 2]" :key="`g${t}`"
            :x1="xPull(t)" :x2="xPull(t)" :y1="TOP" :y2="TOP + rows.length * ROW" class="grid" />
      <line v-for="t in impTicks" :key="`gi${t}`"
            :x1="xImp(t)" :x2="xImp(t)" :y1="TOP" :y2="TOP + rows.length * ROW" class="grid" />

      <g class="frame">
        <rect :x="PULL0" :y="TOP" :width="PULL1 - PULL0" :height="rows.length * ROW" />
        <rect :x="IMP0" :y="TOP" :width="IMP1 - IMP0" :height="rows.length * ROW" />
      </g>

      <g class="tick">
        <text v-for="t in [-2, -1, 0, 1, 2]" :key="`t${t}`"
              :x="xPull(t)" :y="TOP + rows.length * ROW + 14">{{ t }}</text>
        <text v-for="t in impTicks" :key="`ti${t}`"
              :x="xImp(t)" :y="TOP + rows.length * ROW + 14">{{ fmt(t) }}</text>
      </g>

      <text :x="(PULL0 + PULL1) / 2" :y="TOP + rows.length * ROW + 32" class="axis">
        (θ̂−θ<tspan class="sub">I</tspan>)/σ<tspan class="sub">I</tspan>
        <tspan class="axis-pull">  (θ̂−θ<tspan class="sub">I</tspan>)/√(σ<tspan
          class="sub">I</tspan>²−σ²)</tspan>
      </text>
      <text :x="(IMP0 + IMP1) / 2" :y="TOP + rows.length * ROW + 32" class="axis">
        Δ{{ poi }}̂
      </text>

      <g class="legend" :transform="`translate(0 ${TOP + rows.length * ROW + 52})`">
        <line :x1="RANK" :x2="RANK + 22" y1="-4" y2="-4" class="bar" />
        <circle :cx="RANK + 11" cy="-4" r="2.6" class="dot" />
        <text :x="RANK + 28" y="0">Fit</text>
        <path :d="cross(RANK + 11, 12)" class="pull" />
        <text :x="RANK + 28" y="16">Pull</text>
        <rect :x="RANK + 76" y="-11" width="16" height="12" class="hi" />
        <text :x="RANK + 98" y="0">+1σ impact</text>
        <rect :x="RANK + 76" y="5" width="16" height="12" class="lo" />
        <text :x="RANK + 98" y="16">−1σ impact</text>
      </g>

      <!-- The POI has no row of its own, so its readout is its selector. -->
      <g class="pick" @click="$emit('select', null)">
        <rect v-if="selected === null" :x="IMP1 - 250" :y="8" :width="254" :height="26"
              class="chip" />
        <rect v-else :x="IMP1 - 250" :y="8" :width="254" :height="26" fill="transparent" />
        <text :x="IMP1 - 6" :y="26" class="best">
          {{ poi }}̂ = {{ poiFit[1].toFixed(3) }}
          <tspan class="err">
            +{{ (poiFit[2] - poiFit[1]).toFixed(3) }} −{{ (poiFit[1] - poiFit[0]).toFixed(3) }}
          </tspan>
        </text>
      </g>
      <text :x="PULL1" :y="26" class="hint">click a row to profile it</text>
    </svg>

    <p v-if="hidden > 0 || flagged > 0" class="more">
      <template v-if="hidden > 0">{{ hidden }} more with smaller impacts, in the download.</template>
      <template v-if="flagged > 0">
        † {{ flagged }} shown here needed an approximation — a crossing search or a re-fit did not
        converge, so a shift of zero there means unknown, not negligible.
      </template>
    </p>
  </figure>
</template>

<script lang="ts">
import { computed, defineComponent, ref, type PropType } from "vue";
import type { Impact } from "@/types";

const W = 920;
const ROW = 22;
const TOP = 46;
const BOT = 86;
const RANK = 28;
const NAME = 370;
const PULL0 = 382;
const PULL1 = 686;
const IMP0 = 700;
const IMP1 = 906;

/// Combine hard-codes the pull axis to +-2.9 rather than fitting it to the data.
const PULL_MAX = 2.9;

/// Combine's own map from constraint type to label colour.
const COLOURS: Record<string, string> = {
  Gaussian: "#000000",
  Poisson: "#59D454",
  AsymmetricGaussian: "#5954D9",
  Unconstrained: "#80809C",
};

export default defineComponent({
  props: {
    impacts: { type: Array as PropType<Impact[]>, required: true },
    poi: { type: String, default: "r" },
    /// `[best + lo, best, best + hi]`, as Combine writes the POI triplet.
    poiFit: { type: Array as unknown as PropType<[number, number, number]>, required: true },
    /// Which row the profile plot is showing; null is the POI.
    selected: { type: String as PropType<string | null>, default: null },
    max: { type: Number, default: 30 },
  },
  emits: ["select"],
  setup(props) {
    const sort = ref<"impact" | "constraint" | "pull">("impact");

    /// Every derived quantity plotImpacts.py computes, with its sign rules kept.
    const derived = computed(() =>
      props.impacts.map((p) => {
        const [preLo, pre, preHi] = p.prefit;
        const [fitLo, fit, fitHi] = p.fit;
        const eHi = preHi - pre;
        const eLo = pre - preLo;
        const unconstrained = p.kind === "Unconstrained" || (eHi === 0 && eLo === 0);

        // Each offset is divided by the pre-fit error on its own side.
        const scale = (v: number) => (v >= 0 ? v / (eHi || 1) : v / (eLo || 1));
        const scFit = scale(fit - pre);

        // The pull is only defined where the data actually constrained the
        // nuisance; Combine hides the marker otherwise rather than faking it.
        const up = fit <= pre;
        const sx = up ? fitHi - fit : fit - fitLo;
        const sx0 = up ? eHi : eLo;
        const ok = sx0 > 0 && sx < sx0 && Math.abs((sx * sx - sx0 * sx0) / (sx0 * sx0)) >= 0.001;
        const pull = !unconstrained && ok ? (fit - pre) / Math.sqrt(sx0 * sx0 - sx * sx) : null;

        const impHi = p.poi[2] - p.poi[1];
        const impLo = p.poi[0] - p.poi[1];
        return {
          ...p,
          unconstrained,
          scFit,
          scHi: scale(fitHi - pre) - scFit,
          scLo: scFit - scale(fitLo - pre),
          pull,
          impHi,
          impLo,
          // Unconstrained parameters sink to the bottom of a constraint sort,
          // exactly as Combine's sentinel does.
          constraint: unconstrained ? 9999 : (fitHi - fitLo) / (preHi - preLo),
          tip:
            `${p.name} (${p.kind})\n` +
            `post-fit ${fit.toFixed(4)} −${(fit - fitLo).toFixed(4)}` +
            ` / +${(fitHi - fit).toFixed(4)}\n` +
            `impact −1σ ${impLo.toFixed(4)}, +1σ ${impHi.toFixed(4)}` +
            (p.ok ? "" : "\napproximated: did not converge"),
        };
      })
    );

    const sorted = computed(() => {
      const out = [...derived.value];
      if (sort.value === "impact") out.sort((a, b) => b.impact - a.impact);
      else if (sort.value === "pull") {
        out.sort((a, b) => Math.abs(b.pull ?? 0) - Math.abs(a.pull ?? 0));
      } else out.sort((a, b) => a.constraint - b.constraint);
      return out;
    });

    const rows = computed(() => sorted.value.slice(0, props.max));
    const height = computed(() => TOP + Math.max(rows.value.length, 1) * ROW + BOT);

    // Combine rescales this per page, so it is the rows on show that set it.
    const span = computed(() => {
      const m = Math.max(...rows.value.map((r) => Math.max(Math.abs(r.impHi), Math.abs(r.impLo))), 0);
      return (m || 1e-6) * 1.1;
    });
    const impTicks = computed(() => {
      const m = span.value;
      return [-m, -m / 2, 0, m / 2, m].map((t) => Number(t.toPrecision(2)));
    });

    return {
      W, ROW, TOP, RANK, NAME, PULL0, PULL1, IMP0, IMP1, Math,
      sort, sorts: ["impact", "constraint", "pull"] as const,
      rows, height, impTicks,
      top: (i: number) => TOP + i * ROW,
      mid: (i: number) => TOP + i * ROW + ROW / 2,
      // Combine offsets the two markers within the row so they never overlap.
      fitY: (i: number) => TOP + i * ROW + ROW * 0.34,
      pullY: (i: number) => TOP + i * ROW + ROW * 0.72,
      xPull: (v: number) =>
        PULL0 + ((Math.max(-PULL_MAX, Math.min(PULL_MAX, v)) + PULL_MAX) / (2 * PULL_MAX)) *
          (PULL1 - PULL0),
      xImp: (v: number) =>
        IMP0 + ((Math.max(-span.value, Math.min(span.value, v)) + span.value) / (2 * span.value)) *
          (IMP1 - IMP0),
      cross: (x: number, y: number) =>
        `M${x - 3} ${y - 3}L${x + 3} ${y + 3}M${x - 3} ${y + 3}L${x + 3} ${y - 3}`,
      colour: (k: string) => COLOURS[k] ?? "#FF0000",
      fmt: (v: number) => (Math.abs(v) < 1e-12 ? "0" : v.toPrecision(2)),
      hidden: computed(() => Math.max(0, props.impacts.length - rows.value.length)),
      flagged: computed(() => rows.value.filter((r) => !r.ok).length),
    };
  },
});
</script>

<style scoped>
.plot { margin: 0; }
figcaption { font-size: .8rem; color: #6b7280; margin-bottom: .35rem; }
svg { width: 100%; height: auto; background: #fff; border: 1px solid #e5e7eb; border-radius: .375rem; }

.sort {
  font-size: .7rem; text-transform: capitalize; padding: .1rem .4rem;
  border: 1px solid #e5e7eb; border-radius: .25rem; color: #6b7280; background: #fff;
}
.sort.on { background: #f3f4f6; color: #111827; border-color: #d1d5db; }

.band { fill: #e5e5e5; }
.mask { fill: #fff; }
.grid { stroke: #9ca3af; stroke-width: .6; stroke-dasharray: 2 3; }
.frame rect { fill: none; stroke: #6b7280; stroke-width: .8; }

.bar { stroke: #000; stroke-width: 1.4; }
.dot { fill: #000; }
.pull { stroke: #0000ff; stroke-width: 1.4; fill: none; }
/* plotImpacts.py's own palette, at its own 0.7 alpha. */
.hi { fill: #cf5e61; fill-opacity: .7; }
.lo { fill: #7d99d1; fill-opacity: .7; }

.rank { font-size: 11px; fill: #6b7280; text-anchor: end; }
.name { font-size: 11px; text-anchor: end; }
.absolute { font-size: 11px; fill: #374151; text-anchor: middle; }
.tick { font-size: 11px; fill: #6b7280; text-anchor: middle; }
.axis { font-size: 13px; fill: #374151; text-anchor: middle; }
.axis-pull { fill: #0000ff; }
.sub { font-size: 9px; baseline-shift: sub; }
.legend text { font-size: 12px; fill: #374151; }
.best { font-size: 16px; fill: #111827; text-anchor: end; }
.best .err { font-size: 12px; fill: #4b5563; }

/* Rows and the POI readout are controls: clicking one profiles it. */
.pick { cursor: pointer; }
.pick:hover .name, .pick:hover .rank { fill: #111827; }
.sel { fill: #dbeafe; }
.chip { fill: none; stroke: #93c5fd; stroke-width: 1.5; rx: 4; }
.hint { font-size: 11px; fill: #9ca3af; text-anchor: end; }
.more { font-size: .75rem; color: #9ca3af; margin: .35rem 0 0; }
</style>
