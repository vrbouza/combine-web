import { Fitter } from "./Fitter";
import type { Anchor, Config } from "@/types";

interface FitMessage {
  fit: true;
  card: string;
  files: Array<[string, Uint8Array]>;
  cfg: Config;
}

/// Build a model and hold it, so this worker can then serve impact slices.
interface ImpactsInitMessage {
  impactsInit: true;
  card: string;
  files: Array<[string, Uint8Array]>;
  cfg: Config;
}

interface ImpactsRunMessage {
  impactsRun: true;
  anchor: Anchor;
  which: number[];
  asymmetric: boolean;
}

/// Profile one parameter using the model the last fit left behind.
interface ScanParamMessage {
  scanParam: true;
  index: number;
  points: number;
}

type Message =
  | FitMessage
  | ImpactsInitMessage
  | ImpactsRunMessage
  | ScanParamMessage
  | { dispose: true };

const ctx: Worker = self as unknown as Worker;
const fitter = new Fitter(ctx);

ctx.onmessage = (evt: MessageEvent<Message>) => {
  if (!(evt.data instanceof Object)) return;
  const m = evt.data;
  if ("fit" in m && m.fit) {
    void fitter.run(m.card, new Map(m.files), m.cfg);
  } else if ("impactsInit" in m && m.impactsInit) {
    void fitter.init(m.card, new Map(m.files), m.cfg);
  } else if ("impactsRun" in m && m.impactsRun) {
    fitter.impacts(m.anchor, m.which, m.asymmetric);
  } else if ("scanParam" in m && m.scanParam) {
    fitter.scanParam(m.index, m.points);
  } else if ("dispose" in m && m.dispose) {
    fitter.dispose();
  } else {
    throw new Error(`unsupported worker message: ${JSON.stringify(evt.data)}`);
  }
};
