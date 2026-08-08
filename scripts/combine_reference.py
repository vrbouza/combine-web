#!/usr/bin/env python3
"""Dump per-bin expected yields from a real Combine workspace.

This is the gate for template morphing. A fit mismatch is undebuggable; a
per-bin mismatch names the process and the bin, so compare here first.

Yields come from `generateBinned(obs, ExpectedData(), Extended())` on each
channel pdf, which is RooFit's own expected-data path and therefore exercises
whatever morphing and normalisation Combine actually built.

Run inside the Combine environment:

    conda activate combine
    python3 scripts/combine_reference.py <card.txt> [--mass 200] [--points 12]
"""

import argparse
import json
import pathlib
import random
import shutil
import subprocess
import sys
import tempfile


def build_workspace(card: pathlib.Path, mass: str, work: pathlib.Path) -> pathlib.Path:
    shutil.copy(card, work / card.name)
    for line in card.read_text().splitlines():
        f = line.split("#", 1)[0].split()
        if not f or f[0] != "shapes":
            continue
        # The filename slot takes wildcards, as in `..._$CHANNEL.root`.
        for tok in f[1:]:
            if not tok.endswith(".root"):
                continue
            import re

            for src in card.parent.glob(re.sub(r"\$[A-Z]+", "*", tok)):
                if not (work / src.name).exists():
                    shutil.copy(src, work / src.name)

    out = work / "ws.root"
    p = subprocess.run(
        ["text2workspace.py", card.name, "-o", "ws.root", "--mass", mass],
        cwd=work, capture_output=True, text=True,
    )
    if p.returncode != 0:
        sys.exit(f"text2workspace.py failed:\n{p.stdout}\n{p.stderr}")
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("card", type=pathlib.Path)
    ap.add_argument("--mass", default="120")
    ap.add_argument("--points", type=int, default=12, help="random points after the nominal one")
    ap.add_argument("--seed", type=int, default=20260806)
    ap.add_argument("-o", "--output", default="reference.json")
    ap.add_argument("--check", action="store_true", help="run kestrel and diff against it")
    ap.add_argument("--ignore-shapes", action="store_true", help="pass --ignore-shapes to kestrel")
    ap.add_argument("--tol", type=float, default=1e-12)
    ap.add_argument("--one-at-a-time", action="store_true",
                    help="one point per parameter, moved alone")
    args = ap.parse_args()

    import ROOT

    ROOT.gROOT.SetBatch(True)

    work = pathlib.Path(tempfile.mkdtemp(prefix="kestrel_ref_"))
    ws_path = build_workspace(args.card.resolve(), args.mass, work)

    f = ROOT.TFile.Open(str(ws_path))
    w = f.Get("w")
    mc = w.genobj("ModelConfig")
    pdf = mc.GetPdf()
    all_obs = mc.GetObservables()

    pois = [p.GetName() for p in mc.GetParametersOfInterest()]
    nuis = [p.GetName() for p in (mc.GetNuisanceParameters() or [])]
    # Free parameters (rateParam, flatParam) are not in the nuisance set but do
    # change the yields, so sweep everything floating.
    floating = [
        v.GetName()
        for v in w.allVars()
        if not v.isConstant() and v.GetName() not in all_obs.contentsString().split(",")
    ]
    params = pois + [p for p in sorted(set(nuis) | set(floating)) if p not in pois]
    params = [p for p in params if w.var(p) is not None]

    cat = None
    for o in all_obs:
        if o.InheritsFrom("RooCategory"):
            cat = o
    # RooCategory labels come back as std::string, which is not a valid JSON key.
    channels = [str(c.first) for c in cat] if cat else ["ch1"]

    def expected_now():
        out = {}
        for ch in channels:
            cp = pdf.getPdf(ch) if cat else pdf
            obs = cp.getObservables(all_obs)
            if cat:
                found = obs.find(cat.GetName())
                if found:
                    obs.remove(found)
            dh = cp.generateBinned(obs, ROOT.RooFit.ExpectedData(), ROOT.RooFit.Extended())
            out[ch] = [dh.weight(i) for i in range(dh.numEntries())]
        return out

    def norms_now():
        out = {}
        for fn in w.allFunctions():
            n = fn.GetName()
            if n.startswith("n_exp_"):
                out[n] = fn.getVal()
        return out

    nominal = {p: w.var(p).getVal() for p in params}
    rng = random.Random(args.seed)

    # `--one-at-a-time` replaces the random sweep with one point per parameter,
    # each moved alone. When something disagrees, this says which parameter.
    if args.one_at_a_time:
        movable = [p for p in params if not p.startswith("prop_bin")]
        points = []
        for p in [None] + movable:
            values = dict(nominal)
            if p is not None:
                v = w.var(p)
                step = 1.0 if v.getMax() - v.getMin() > 4.0 else 0.4
                values[p] = min(nominal[p] + step, v.getMax() - 1e-9)
            for q, x in values.items():
                w.var(q).setVal(x)
            points.append({"values": values, "norms": norms_now(), "expected": expected_now(),
                           "moved": p or "(nominal)"})
        for q, x in nominal.items():
            w.var(q).setVal(x)
        ref = {"card": str(args.card), "mass": args.mass, "parameters": params,
               "channels": {ch: len(points[0]["expected"][ch]) for ch in channels},
               "points": points}
        pathlib.Path(args.output).write_text(json.dumps(ref, indent=1))
        print(f"{args.output}: {len(points)} points, one parameter each", file=sys.stderr)
        return check(args, ref) if args.check else 0

    points = []
    for i in range(args.points + 1):
        if i == 0:
            values = dict(nominal)  # the pre-fit point
        else:
            values = {}
            for p in params:
                v = w.var(p)
                # Barlow-Beeston parameters are solved analytically on our side,
                # so randomising them would compare two different things. Held
                # at nominal, both sides agree they contribute nothing.
                if p.startswith("prop_bin"):
                    values[p] = nominal[p]
                    continue
                if p in pois:
                    x = rng.uniform(0.2, 2.5)
                elif v.hasMin() and v.hasMax() and v.getMax() - v.getMin() < 4.0:
                    x = rng.uniform(v.getMin(), v.getMax())
                else:
                    # Unit-Gaussian nuisances; +-2 keeps morphing inside the
                    # smooth region as well as outside it.
                    x = nominal[p] + rng.gauss(0.0, 1.0) * (2.0 if abs(nominal[p]) < 1e-9 else 0.2)
                lo, hi = v.getMin(), v.getMax()
                values[p] = min(max(x, lo + 1e-9), hi - 1e-9)

        for p, x in values.items():
            w.var(p).setVal(x)
        points.append({"values": values, "norms": norms_now(), "expected": expected_now()})

    for p, x in nominal.items():
        w.var(p).setVal(x)

    ref = {
        "card": str(args.card),
        "mass": args.mass,
        "parameters": params,
        "channels": {ch: len(points[0]["expected"][ch]) for ch in channels},
        "points": points,
    }
    pathlib.Path(args.output).write_text(json.dumps(ref, indent=1))
    print(
        f"{args.output}: {len(points)} points, "
        f"{len(params)} parameters, channels {ref['channels']}",
        file=sys.stderr,
    )
    return check(args, ref) if args.check else 0


def check(args, ref) -> int:
    """Evaluate the same points with kestrel and diff per bin."""
    kestrel = pathlib.Path(__file__).resolve().parent.parent / "rust/kestrel/target/release/kestrel"
    cmd = [str(kestrel), "yields", str(args.card), "--mass", args.mass, "--at", args.output]
    if args.ignore_shapes:
        cmd.append("--ignore-shapes")
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"kestrel failed:\n{p.stderr}")
    got = json.loads(p.stdout)

    print(f"\n{'point':>34}  {'worst |diff|':>13}  {'worst rel':>11}  where")
    worst_all = 0.0
    for i, (want_pt, got_pt) in enumerate(zip(ref["points"], got["points"])):
        worst, worst_rel, where = 0.0, 0.0, ""
        for ch, want in want_pt["expected"].items():
            mine = got_pt["expected"].get(ch)
            if mine is None:
                sys.exit(f"kestrel has no channel {ch}")
            # `CMS_th1x` is one observable shared by every channel, sized for the
            # largest, so a shorter channel comes back zero-padded.
            if len(want) > len(mine):
                tail = want[len(mine):]
                # The padding is pdf-normalisation leakage, not a real yield, so
                # judge it against the channel's own scale.
                floor = 1e-6 * max(max(map(abs, want[: len(mine)])), 1.0)
                if any(abs(v) > floor for v in tail):
                    sys.exit(f"channel {ch}: reference has {len(want)} bins, kestrel {len(mine)}, "
                             f"and the extra bins are not empty (max {max(map(abs, tail)):.3e})")
                want = want[: len(mine)]
            elif len(want) < len(mine):
                sys.exit(f"channel {ch}: {len(want)} bins in the reference, {len(mine)} in kestrel")
            for b, (a, c) in enumerate(zip(want, mine)):
                d = abs(a - c)
                if d > worst:
                    worst, worst_rel, where = d, d / max(abs(a), 1e-30), f"{ch} bin {b + 1}"
        worst_all = max(worst_all, worst)
        label = want_pt.get("moved", str(i))
        print(f"{label:>34}  {worst:>13.3e}  {worst_rel:>11.3e}  {where}")

    print(f"\nworst over all points: {worst_all:.3e}   tolerance {args.tol:.1e}")
    ok = worst_all < args.tol
    print("GATE: PASS" if ok else "GATE: FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
