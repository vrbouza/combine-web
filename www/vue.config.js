const path = require("path");
const { defineConfig } = require("@vue/cli-service");
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");

module.exports = defineConfig({
  publicPath: "/",

  configureWebpack: {
    experiments: {
      // Required for `await import("@/pkg_kestrel")` to resolve the wasm module.
      asyncWebAssembly: true,
    },
    devServer: {
      // Rebuilding the wasm crate on every keystroke makes the dev server
      // unusable, so watching and hot reload are off. Reload by hand after a
      // Rust edit.
      hot: false,
      liveReload: false,
      watchFiles: { paths: [] },
    },
  },

  chainWebpack: (config) => {
    // The wasm interface is gated on `cfg(target_family = "wasm")`, not on a
    // cargo feature, so wasm-pack needs no extraArgs: the target alone selects it.
    config
      .plugin("wasm-pack_kestrel")
      .use(WasmPackPlugin)
      .init(
        (Plugin) =>
          new Plugin({
            crateDirectory: path.resolve(__dirname, "../rust/kestrel"),
            outDir: path.resolve(__dirname, "./src/pkg_kestrel"),
            forceMode: "production",
          })
      )
      .end();

    // Worker entrypoints must bypass the default js/ts rules and go through
    // worker-loader, which turns the default export into a `new Worker()`.
    config.module.rule("js").exclude.add(/\.worker\.(js|ts)$/);
    config.module.rule("ts").exclude.add(/\.worker\.ts$/);

    config.module
      .rule("worker")
      .test(/\.worker\.(js|ts)$/)
      .use("worker-loader")
      .loader("worker-loader")
      .end()
      .use("ts-loader")
      .loader("ts-loader")
      .options({ transpileOnly: true, appendTsSuffixTo: [/\.vue$/] })
      .end();

    config.plugin("define").tap((definitions) => {
      Object.assign(definitions[0], {
        __VUE_OPTIONS_API__: "true",
        __VUE_PROD_DEVTOOLS__: "false",
        __VUE_PROD_HYDRATION_MISMATCH_DETAILS__: "false",
      });
      return definitions;
    });
  },
});
