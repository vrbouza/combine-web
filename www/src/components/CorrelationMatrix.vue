<template>
  <figure class="plot bg-white border border-gray-200 rounded-md p-4 mb-4">
    <figcaption>
      Correlation matrix. Blue is positive, red negative. Strongest off-diagonal pairs are listed below.
    </figcaption>
    <svg :viewBox="`0 0 ${side} ${side}`" role="img" aria-label="Correlation matrix">
      <rect
        v-for="c in cells"
        :key="c.k"
        :x="c.x" :y="c.y" :width="cell" :height="cell"
        :fill="colour(c.v)"
      >
        <title>{{ c.t }}</title>
      </rect>
    </svg>
    <ul v-if="strongest.length" class="pairs">
      <li v-for="p in strongest" :key="p.k">
        <code>{{ p.a }}</code> &times; <code>{{ p.b }}</code>
        <span :class="p.v > 0 ? 'pos' : 'neg'">{{ p.v.toFixed(3) }}</span>
      </li>
    </ul>
  </figure>
</template>

<script lang="ts">
import { computed, defineComponent, type PropType } from "vue";

export default defineComponent({
  props: {
    names: { type: Array as PropType<string[]>, required: true },
    correlation: { type: Array as PropType<number[]>, required: true },
  },
  setup(props) {
    const n = computed(() => props.names.length);
    const cell = computed(() => Math.max(2, Math.min(18, 360 / Math.max(n.value, 1))));
    const side = computed(() => cell.value * n.value);

    const cells = computed(() => {
      const out = [];
      for (let i = 0; i < n.value; i++) {
        for (let j = 0; j < n.value; j++) {
          const v = props.correlation[i * n.value + j] ?? 0;
          out.push({
            k: `${i}-${j}`,
            x: j * cell.value,
            y: i * cell.value,
            v,
            t: `${props.names[i]} x ${props.names[j]} = ${v.toFixed(3)}`,
          });
        }
      }
      return out;
    });

    const strongest = computed(() => {
      const out = [];
      for (let i = 0; i < n.value; i++) {
        for (let j = i + 1; j < n.value; j++) {
          const v = props.correlation[i * n.value + j] ?? 0;
          out.push({ k: `${i}-${j}`, a: props.names[i], b: props.names[j], v });
        }
      }
      return out.sort((p, q) => Math.abs(q.v) - Math.abs(p.v)).slice(0, 6);
    });

    return {
      cell, side, cells, strongest,
      // Diverging blue/red through white, so zero reads as blank.
      colour: (v: number) => {
        const a = Math.min(Math.abs(v), 1);
        return v >= 0
          ? `rgb(${Math.round(255 - 140 * a)},${Math.round(255 - 90 * a)},255)`
          : `rgb(255,${Math.round(255 - 120 * a)},${Math.round(255 - 120 * a)})`;
      },
    };
  },
});
</script>

<style scoped>
.plot { margin: 0; }
figcaption { font-size: .8rem; color: #6b7280; margin-bottom: .25rem; }
/* No text lives in this SVG, so it scales for free. The cap is still needed
   because the matrix is square: below the two-column breakpoint its column is
   the full results width, and without it this becomes a ~1000px block. */
svg { width: 100%; max-width: 460px; height: auto; border: 1px solid #e5e7eb; border-radius: .375rem; background: #fff; }
.pairs { list-style: none; padding: 0; margin: .5rem 0 0; font-size: .875rem; }
.pairs li { padding: .1rem 0; }
.pairs code { font-size: .8rem; color: #374151; }
.pos { color: #36c; margin-left: .4rem; }
.neg { color: #c33; margin-left: .4rem; }
</style>
