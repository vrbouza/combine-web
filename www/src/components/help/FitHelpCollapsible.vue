<template>
  <Collapsible v-model:open="isOpen" class="border border-gray-200 rounded-lg bg-white mb-4">
    <CollapsibleTrigger
      class="flex items-center justify-between w-full p-3 hover:bg-gray-50 rounded-lg"
    >
      <div class="flex items-center gap-2">
        <HelpCircle class="w-4 h-4 text-gray-500" />
        <span class="font-medium text-sm">How to use the fit</span>
      </div>
      <ChevronDown
        class="w-4 h-4 text-gray-500 transition-transform duration-200"
        :class="{ 'rotate-180': isOpen }"
      />
    </CollapsibleTrigger>

    <CollapsibleContent class="px-3 pb-3">
      <Tabs default-value="overview" class="mt-2">
        <TabsList class="grid w-full grid-cols-3 h-8">
          <TabsTrigger value="overview" class="text-xs">Overview</TabsTrigger>
          <TabsTrigger value="parameters" class="text-xs">Parameters</TabsTrigger>
          <TabsTrigger value="reading" class="text-xs">Reading</TabsTrigger>
        </TabsList>

        <div class="mt-3 text-sm text-gray-600 max-h-64 overflow-y-auto">
          <TabsContent value="overview" class="space-y-4">
            <p>
              Drop a
              <a
                href="https://cms-analysis.github.io/HiggsAnalysis-CombinedLimit/latest/part2/settinguptheanalysis/"
                target="_blank"
                class="text-blue-600 hover:underline"
                >Combine datacard</a
              >
              and any <code>.root</code> files it names. The maximum-likelihood fit runs here, in
              your browser, compiled to WebAssembly.
            </p>

            <p>
              This is a re-implementation of Combine's binned likelihood, not a port — ROOT cannot
              be compiled to WebAssembly. It is validated against Combine 11.0.0 and agrees to
              about 1e-6 on the fitted parameter of interest.
            </p>

            <div>
              <p class="font-medium text-gray-900 mb-2">Supported</p>
              <p>
                Counting and TH1 shape datacards, with <code>lnN</code>, <code>lnU</code>,
                <code>gmN</code>, <code>shape</code>/<code>shapeN</code>, <code>param</code>,
                <code>rateParam</code>, <code>flatParam</code> and <code>autoMCStats</code>.
                Anything else is refused by name rather than quietly mis-fitted.
              </p>
            </div>

            <div>
              <p class="font-medium text-gray-900 mb-2">Example data</p>
              <ul class="list-disc list-inside mt-1">
                <li>
                  <a href="samples/simple-shapes-TH1.txt" class="text-blue-600 hover:underline"
                    >simple-shapes-TH1.txt</a
                  >
                  with
                  <a
                    href="samples/simple-shapes-TH1_input.root"
                    class="text-blue-600 hover:underline"
                    >its histograms</a
                  >
                </li>
                <li>
                  <a href="samples/counting.txt" class="text-blue-600 hover:underline"
                    >counting.txt</a
                  >
                  — no histograms needed
                </li>
              </ul>
            </div>
          </TabsContent>

          <TabsContent value="parameters">
            <dl class="space-y-3">
              <div>
                <dt class="font-medium text-gray-900">r min / r max</dt>
                <dd class="ml-4">
                  Range the signal strength is allowed to take, matching Combine's
                  <code>--rMin</code>/<code>--rMax</code>. It defaults to [−5, 5]: letting it go
                  negative avoids the one-sided impacts Combine's own documentation warns about.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Mass</dt>
                <dd class="ml-4">
                  Substituted for <code>$MASS</code> in the card's <code>shapes</code> patterns, so
                  mass-suffixed templates such as <code>ggH200</code> resolve.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">MINOS</dt>
                <dd class="ml-4">
                  Asymmetric errors from the profile likelihood rather than the parabolic HESSE
                  approximation. If the crossing search fails it falls back to the scan, as
                  Combine's <code>--robustFit</code> does.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Scan points</dt>
                <dd class="ml-4">
                  How many points in the profile-likelihood scan. Each is a full minimisation with
                  the parameter of interest fixed, so this dominates the running time. Set it to 0
                  to skip the scan.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Compute impacts</dt>
                <dd class="ml-4">
                  Ranks the nuisances by their effect on the parameter of interest. Each one is
                  fixed at its post-fit &plusmn;1&sigma; while everything else, the POI included, is
                  re-profiled; the resulting shift is its impact. That is two extra fits per
                  nuisance, so it runs across a pool of web workers and is off by default. The
                  impact measures how strongly a nuisance is correlated with the POI &mdash; it is
                  <em>not</em> a decomposition of the uncertainty, and the shifts do not add in
                  quadrature to the total.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Asymmetric errors</dt>
                <dd class="ml-4">
                  Displaces each nuisance by its own MINOS crossings, which is what Combine does by
                  default. Switching it off uses the symmetric HESSE error instead: faster, and the
                  fallback Combine itself takes when a crossing search leaves no interval. The
                  impacts stay asymmetric either way, because the two re-fits are independent.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Threads</dt>
                <dd class="ml-4">
                  How many web workers share the impact fits. Each holds its own copy of the model
                  and its own WebAssembly instance, so more threads cost memory as well as cores.
                  Returns fall off quickly: 72 nuisances took 5.3 s on one worker and 1.9 s on
                  eight.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Ignore shape systematics</dt>
                <dd class="ml-4">
                  Skips template morphing entirely and fits normalisation systematics only. Much
                  faster, but the result is <em>less</em> constrained than Combine's, so the
                  interval comes out narrower and is not comparable.
                </dd>
              </div>
            </dl>
          </TabsContent>

          <TabsContent value="reading" class="space-y-4">
            <p>
              The impacts plot carries three different quantities. They are independent: a nuisance
              can move a long way and leave the POI untouched, or sit exactly where its prior put it
              and still dominate the uncertainty.
            </p>
            <dl class="space-y-3">
              <div>
                <dt class="font-medium text-gray-900">Pull — did it move?</dt>
                <dd class="ml-4">
                  The black point is <code>(θ̂ − θ₁)/σ₁</code>: how far the nuisance ended up from
                  its pre-fit centre, in units of the <em>pre-fit</em> width. The blue
                  <code>×</code> is <code>(θ̂ − θ₁)/√(σ₁² − σ²)</code>, the same shift over the
                  reduction in width — the statistically correct pull once the data constrains the
                  nuisance, since the first form then over-covers. It is undefined, and hidden, when
                  the data did not constrain it at all.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Constraint — did we learn anything?</dt>
                <dd class="ml-4">
                  <code>σ/σ₁</code>, post-fit width over pre-fit width, drawn as the <em>length</em>
                  of the black bar rather than its position. Below 1 means the data pinned the
                  nuisance down harder than its prior did; exactly 1 means the data said nothing
                  about it. Click the row to see this directly: the profile is the bare prior,
                  crossing 68% at ±1, unless the data narrowed it.
                </dd>
              </div>
              <div>
                <dt class="font-medium text-gray-900">Impact — does the POI care?</dt>
                <dd class="ml-4">
                  The coloured bars, <code>r̂(θ̂ ± σ) − r̂</code>: the shift in the POI when the
                  nuisance is fixed at its post-fit ±1σ and everything else is re-profiled. It
                  shares no arithmetic with the other two and costs two extra fits per nuisance. It
                  is essentially a correlation — in the linear approximation
                  <code>impact ≈ σ_r · ρ(r, θ)</code>. The two bars often point the same way,
                  because they come from independent re-fits.
                </dd>
              </div>
            </dl>
            <p>
              Impacts do <strong>not</strong> add in quadrature to the total uncertainty. For a
              breakdown that does, Combine's own recipe is to freeze groups of nuisances in
              sequence and subtract in quadrature.
            </p>
          </TabsContent>
        </div>
      </Tabs>
    </CollapsibleContent>
  </Collapsible>
</template>

<script lang="ts">
import { defineComponent, ref } from "vue";
import { ChevronDown, HelpCircle } from "@lucide/vue";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

export default defineComponent({
  components: {
    ChevronDown,
    HelpCircle,
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
    Tabs,
    TabsContent,
    TabsList,
    TabsTrigger,
  },
  setup() {
    const isOpen = ref(false);
    return { isOpen };
  },
});
</script>
