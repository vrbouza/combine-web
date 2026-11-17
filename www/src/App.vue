<template>
  <div class="min-h-svh">
    <main class="mx-auto max-w-[1920px] px-4 py-8 md:px-8 md:py-10">
      <FitPage />

      <footer class="mt-12 max-w-3xl border-t border-gray-200 pt-4 text-xs text-gray-500">
        Everything runs in this site: no datacard or histogram is uploaded. This is a re-implementation of the subset of
        <a
          href="https://cms-analysis.github.io/HiggsAnalysis-CombinedLimit/latest/"
          target="_blank"
          class="text-blue-600 hover:underline"
          >CMS Combine</a
        >
        needed for a single-POI binned fit, validated against it. It is NOT a full port of CMS Combine.
      </footer>
    </main>
  </div>
</template>

<script lang="ts">
import { defineComponent, onMounted } from "vue";
import { useStore } from "vuex";
import WorkerFitter from "@/workers/Fitter.worker";
import FitPage from "@/components/pages/FitPage.vue";

export default defineComponent({
  components: { FitPage },
  setup() {
    const store = useStore();
    onMounted(() => {
      if (!window.Worker) throw new Error("this browser does not support Web Workers");
      // One worker for the session: it owns the wasm module, so keeping it alive
      // means the module is compiled once rather than per fit.
      store.commit("SET_WORKER", new WorkerFitter());
    });
  },
});
</script>
