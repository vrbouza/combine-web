export type ErrorKind = "parse" | "shapes" | "converge" | "memory" | "generic";

export interface FittedParam {
  name: string;
  value: number;
  error: number;
}

export interface FitResult {
  params: FittedParam[];
  poi: FittedParam;
  minos: [number, number] | null;
  nll: number;
  edm: number;
  nfcn: number;
  valid: boolean;
  cascadeStep: number;
}

export interface ProcessSummary {
  name: string;
  isSignal: boolean;
  rate: number;
}

export interface ChannelSummary {
  name: string;
  nBins: number;
  processes: ProcessSummary[];
}

export interface Summary {
  channels: ChannelSummary[];
  nParams: number;
  nBins: number;
  ignoredShapeSysts: string[];
  expected: number[];
  observed: number[];
}

export interface ScanPoint {
  r: number;
  deltaNll: number;
  nll: number;
  valid: boolean;
}

export interface ScanResult {
  bestFit: number;
  bestNll: number;
  points: ScanPoint[];
  crossings68: [number, number] | null;
  crossings95: [number, number] | null;
}

export interface Pull {
  name: string;
  pull: number;
  constraint: number;
  value: number;
  error: number;
  constrained: boolean;
}

export interface Report {
  pulls: Pull[];
  names: string[];
  correlation: number[];
}

/// One nuisance's impact, in the triplet layout of Combine's `impacts.json`.
export interface Impact {
  name: string;
  kind: string;
  /// `[centre - wLo, centre, centre + wHi]` from the prior alone.
  prefit: [number, number, number];
  /// `[thetaHat - sigmaLo, thetaHat, thetaHat + sigmaHi]`.
  fit: [number, number, number];
  /// `[r(theta-), rHat, r(theta+)]`.
  poi: [number, number, number];
  impact: number;
  /// False when a crossing search or a re-fit had to be approximated. Such a
  /// row is drawn, but marked: a shift we could not compute must not read as a
  /// genuine zero.
  ok: boolean;
}

export interface Nuisance {
  index: number;
  name: string;
}

/// The global-fit state the per-nuisance fits are measured against. Built on
/// the main thread from the fit result and broadcast to every pool worker.
export interface Anchor {
  best: number[];
  error: number[];
  poi: number;
}

export interface Config {
  rMin: number;
  rMax: number;
  mass: string;
  minos: boolean;
  ignoreShapes: boolean;
  scanPoints: number;
  computeImpacts: boolean;
  /// MINOS crossings per nuisance, as Combine does by default, rather than the
  /// symmetric HESSE error.
  impactsAsymmetric: boolean;
  /// Size of the impacts worker pool. Each worker holds its own model and its
  /// own wasm instance, so this trades memory for wall clock.
  impactWorkers: number;
}

export function defaultConfig(): Config {
  const cores = typeof navigator === "undefined" ? 4 : navigator.hardwareConcurrency || 4;
  return {
    rMin: -5, rMax: 5, mass: "120", minos: true, ignoreShapes: false, scanPoints: 40,
    computeImpacts: true, impactsAsymmetric: true,
    impactWorkers: Math.min(cores, 8),
  };
}

export interface RootState {
  worker: Worker | null;
  running: boolean;
  summary: Summary | null;
  result: FitResult | null;
  scan: ScanResult | null;
  report: Report | null;
  warnings: string[];
  stage: string;
  error: { kind: ErrorKind; detail: string } | null;
  elapsedMs: number;
  impacts: Impact[] | null;
  impactsProgress: { done: number; total: number; workers: number } | null;
  impactsError: string | null;
  nuisances: Nuisance[];
  /// Which parameter the profile plot is showing. Null means the POI.
  selectedParam: Nuisance | null;
  /// Profile scans of individual nuisances, cached by name. Each costs a run of
  /// minimisations and users click back and forth, so they are kept.
  paramScans: Record<string, ScanResult>;
  paramScanRunning: boolean;
}
