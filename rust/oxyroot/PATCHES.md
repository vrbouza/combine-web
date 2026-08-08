# Deviations from upstream `oxyroot`

Upstream: https://github.com/m-dupont/oxyroot (MIT/Apache-2.0, © Mathieu Dupont).

## Inherited from goshawk-web's fork

Swapped every C-backed codec for a pure-Rust one, so the crate cross-compiles to
`wasm32-unknown-unknown`, which has no C toolchain:

| Was | Now |
|---|---|
| `xz2` (liblzma) | `lzma-rs` |
| `lz4` (liblz4) | `lz4_flex` |
| — | `ruzstd` added for Zstd |

Also added `RootFile::open_from_buffer`, since the browser hands us bytes rather
than a path.

## Added here: TH1 reading

Upstream reads only `TTree`/`Branch`. Combine's shape datacards point at `TH1`
histograms, usually inside sub-directories, so both were added.

New:

- `src/rhist/mod.rs` — module root, plus `TArrayD`/`TArrayF` readers
- `src/rhist/axis.rs` — `TAxis` and `TAttAxis`; `edges()`/`centers()` handle
  variable binning
- `src/rhist/th1.rs` — `Th1`, with `H1D`/`H1F` factory wrappers

Modified:

- `src/riofs/key.rs` — replaced the live `todo!()` on the `TDirectoryFile`
  branch of `Key::object` with real sub-directory recursion
- `src/riofs/dir.rs` — split out `read_keys_from_reader`; added `seek_keys()`,
  `set_name_title()`, a `Named` impl, and factory registration
- `src/riofs/file.rs` — added `get_th1()`, `keys_recursive()`,
  `histogram_names()`, and the `walk_dir` path walker
- `src/rbytes/rbuffer.rs` — added `set_pos_to_end_of`, `end_of`,
  `skip_object_any`, `read_vec_f64`, `read_vec_f32`
- `src/rtypes/factory.rs` — registered `TAxis`, `TH1D`, `TH1F`, `TDirectoryFile`
- `src/rvers.rs` — un-commented `H1`, `H1D`, `H1F`, `AXIS`, `ARRAY_D`, `ARRAY_F`

## ROOT format notes worth keeping

1. **Read through the byte count, not the version.** Combine's own test files
   span TH1 v6, v7 and the TH1F v1 layout, and the tail members differ in each
   (`fBinStatErrOpt` arrives in v7, `fStatOverflows` in v8; `TAxis` gains
   `fModLabs` in v10). The reader parses as far as `fSumw2` and then jumps to
   `hdr.pos + 4 + hdr.len`, so any later member is absorbed for free.
2. **`TArrayD`/`TArrayF` carry no header.** ROOT hand-writes `TArray::Streamer`,
   so the payload is a bare `fN: i32` followed by the data. Calling
   `read_header` on one silently eats the first few bytes.
3. **`fFunctions` must be skipped, not parsed.** It is a `TList*` routinely
   holding `TPaveStats`/`TLatex`/`TBox`, which this crate cannot instantiate.
4. **`fBuffer` is a `TStreamerBasicPointer`**: `fBufferSize: i32`, then a
   one-byte `isArray` flag, then the doubles only if that flag is set.
5. **Variable binning is common.** `htt_input.root` has a non-empty `fXbins`, so
   `edges()` must use it; `fXmin`/`fXmax` alone would misplace every edge.
6. **A sub-directory key's payload has no name or title** — unlike the file's
   top directory, a nested `TDirectoryFile` record is offsets only, so the name
   must be copied from the owning `Key`.

## Bin errors, and a deliberate divergence from uproot

ROOT's `TH1::GetBinError` is `fSumw2.fN ? sqrt(fSumw2[bin]) : sqrt(|content|)`.
This fork follows ROOT, which is what Combine sees. Two consequences:

- For **negative** bin contents ROOT gives `sqrt(|v|)` where uproot gives 0.
  This affects 121 histograms in Combine's corpus.
- `data_obs` in `data/tutorials/shapes/simple-shapes-TH1_input.root` genuinely
  contains `fSumw2[1] = NaN`. ROOT and this fork return NaN; uproot masks it to
  0. Callers must therefore reject non-finite template errors themselves —
  `kestrel` does this when compiling a model, because a NaN reaching
  `autoMCStats` would silently produce a NaN likelihood.

One safety deviation from ROOT: ROOT checks only `fSumw2.fN != 0` and would
index past a short `fSumw2`; here the length must cover `fNcells`, as uproot
requires.

## Verification

- 281 658 histograms across 118 files compared against uproot; all matched.
- An independent random sample of 128 histograms across 4 files reproduced this,
  with a worst absolute difference of 0, and the only two disagreements being
  the NaN case above where this fork is the ROOT-faithful one.
- Ten files fail to open at all, on **pre-existing** streamer gaps unrelated to
  histograms (`ESTLTypeFromInteger(44)` and an unregistered `TStreamerLoop`).
  Only one of them contains TH1s.
