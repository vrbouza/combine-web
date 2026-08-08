declare module "*.vue" {
  import type { DefineComponent } from "vue";
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const component: DefineComponent<{}, {}, any>;
  export default component;
}

/** JSROOT ships no type declarations for its ESM subpaths. */
declare module "jsroot";
declare module "jsroot/core";
declare module "jsroot/draw";

/** `vue3-dropzone` ships no type declarations for its composable. */
declare module "vue3-dropzone";
