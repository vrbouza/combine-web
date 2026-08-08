#!/usr/bin/env python3
"""Nuisance impacts from a real Combine workspace, diffed against kestrel.

The gate for `kestrel impacts`. Combine's own definition is the authority: fix
each nuisance at its post-fit +-1 sigma, re-profile everything else, and record
the shift in the POI.

Combine's default minimiser tolerance of 0.1 under-converges badly enough to
swamp the comparison (PRECISION.md section 3), so both sides run at 1e-6.

Do NOT gate on a card carrying `autoMCStats`: those disagree at ~1e-3 for the
unrelated reason in PRECISION.md item 12, and would mask a real impacts bug.

Run inside the Combine environment:

    conda activate combine
    python3 scripts/combine_impacts.py <card.txt> [--mass 200] [--check]
"""

import argparse
import concurrent.futures
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

TOLERANCE = ["--cminDefaultMinimizerTolerance", "1e-6"]


def build_workspace(card: pathlib.Path, mass: str, work: pathlib.Path) -> pathlib.Path:
    shutil.copy(card, work / card.name)
    for line in card.read_text().splitlines():
        f = line.split("#", 1)[0].split()
        if not f or f[0] != "shapes":
            continue
        for tok in f[1:]:
            if not tok.endswith(".root"):
                continue
            for src in card.parent.glob(re.sub(r"\$[A-Z]+", "*", tok)):
                if not (work / src.name).exists():
                    shutil.copy(src, work / src.name)

    p = subprocess.run(
        ["text2workspace.py", card.name, "-o", "ws.root", "--mass", mass],
        cwd=work, capture_output=True, text=True,
    )
    if p.returncode != 0:
        sys.exit(f"text2workspace.py failed:\n{p.stdout}\n{p.stderr}")
    return work / "ws.root"


def run_impacts(work: pathlib.Path, mass: str, r_min: float, r_max: float,
                params: list, poi: str = "r", jobs: int = 8) -> dict:
    """Drive `combine` directly, one job per nuisance.

    Not via `combineTool.py -M Impacts`: that calls `prefit_from_workspace`
    before dispatching anything, and it raises on any card with an unconstrained
    parameter, because `ws.pdf("<name>_Pdf")` is then a C++ null pointer and
    Combine 11.0.0 compares it to `None`. These are the exact `combine` commands
    `Impacts.py` would have issued.
    """
    common = ["-d", "ws.root", "-m", mass, "--rMin", str(r_min), "--rMax", str(r_max)] + TOLERANCE

    init = work / f"higgsCombine_initialFit_Test.MultiDimFit.mH{mass}.root"
    if not init.exists():
        run_one(work, ["combine", "-M", "MultiDimFit", "-n", "_initialFit_Test",
                       "--algo", "singles", "--redefineSignalPOIs", poi] + common)

    todo = [p for p in params
            if not (work / f"higgsCombine_paramFit_Test_{p}.MultiDimFit.mH{mass}.root").exists()]
    print(f"{len(todo)} of {len(params)} nuisances still to fit", file=sys.stderr)
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = {
            pool.submit(run_one, work,
                        ["combine", "-M", "MultiDimFit", "-n", f"_paramFit_Test_{p}",
                         "--algo", "impact", "--redefineSignalPOIs", poi, "-P", p,
                         "--floatOtherPOIs", "1", "--saveInactivePOI", "1"] + common, False): p
            for p in todo
        }
        for n, f in enumerate(concurrent.futures.as_completed(futures), 1):
            if f.result() is not None:
                print(f"  {futures[f]} failed", file=sys.stderr)
            if n % 10 == 0:
                print(f"  {n}/{len(todo)}", file=sys.stderr)
    return collect(work, mass, poi)


def run_one(work: pathlib.Path, cmd: list, fatal: bool = True):
    p = subprocess.run(cmd, cwd=work, capture_output=True, text=True)
    if p.returncode != 0:
        if fatal:
            sys.exit(f"{' '.join(cmd)} failed:\n{p.stdout[-3000:]}\n{p.stderr[-3000:]}")
        return p
    return None


def collect(work: pathlib.Path, mass: str, poi: str) -> dict:
    """Assemble impacts.json ourselves from the per-parameter fit files.

    Combine 11.0.0's own collection stage crashes here: `utils.py:69` compares a
    C++ null pointer to `None`, which newer cppyy refuses. That code only builds
    the `prefit` triplets, which we take from the card's priors anyway, so we
    read the fit outputs directly and leave the installed package untouched.
    """
    import ROOT  # noqa: PLC0415 - only needed on this path, and slow to import

    def singles(path: pathlib.Path, scanned: list, columns: list):
        f = ROOT.TFile(str(path))
        if not f or f.IsZombie():
            return None
        t = f.Get("limit")
        res = {}
        for i, param in enumerate(scanned):
            res[param] = {}
            for col in columns:
                vals = [getattr(evt, col) for evt in t]
                if len(vals) < 1 + len(scanned) * 2:
                    return None
                # Entry 0 is the nominal fit; 2i+1 and 2i+2 the two displacements.
                res[param][col] = [vals[i * 2 + 1], vals[0], vals[i * 2 + 2]]
        return res

    init = singles(work / f"higgsCombine_initialFit_Test.MultiDimFit.mH{mass}.root", [poi], [poi])
    if init is None:
        sys.exit("the initial fit produced no usable output")

    params = []
    for path in sorted(work.glob(f"higgsCombine_paramFit_Test_*.MultiDimFit.mH{mass}.root")):
        name = path.name[len("higgsCombine_paramFit_Test_"):-len(f".MultiDimFit.mH{mass}.root")]
        one = singles(path, [name], [name, poi])
        if one is None:
            print(f"skipping {name}: no usable output", file=sys.stderr)
            continue
        r = one[name][poi]
        params.append({
            "name": name, "type": "Unknown", "groups": [],
            "prefit": [-1.0, 0.0, 1.0], "fit": one[name][name], poi: r,
            f"impact_{poi}": max(abs(r[2] - r[1]), abs(r[0] - r[1])),
        })

    return {"POIs": [{"name": poi, "fit": init[poi][poi]}], "method": "default",
            "params": params}


def run_kestrel(card: pathlib.Path, mass: str, r_min: float, r_max: float,
                work: pathlib.Path) -> dict:
    exe = pathlib.Path(__file__).resolve().parents[1] / "rust/kestrel/target/release/kestrel"
    if not exe.exists():
        sys.exit(f"{exe} is missing; run `cargo build --release` first")
    out = work / "kestrel.json"
    p = subprocess.run(
        [str(exe), "impacts", str(card), "--mass", mass,
         "--r-min", str(r_min), "--r-max", str(r_max), "--json", str(out)],
        capture_output=True, text=True,
    )
    if p.returncode != 0:
        sys.exit(f"kestrel impacts failed:\n{p.stdout}\n{p.stderr}")
    return json.loads(out.read_text())


def compare(ref: dict, ours: dict) -> int:
    poi = ref["POIs"][0]["name"]
    mine = {p["name"]: p for p in ours["params"]}
    rows, worst = [], 0.0
    missing, degenerate, unconverged = [], [], []

    for p in sorted(ref["params"], key=lambda x: -abs(x[f"impact_{poi}"])):
        q = mine.get(p["name"])
        if q is None:
            missing.append(p["name"])
            continue
        # Signed shifts, exactly as plotImpacts.py derives them from the triplets.
        r_ref, r_our = p[poi], q[poi]
        d_hi = (r_our[2] - r_our[1]) - (r_ref[2] - r_ref[1])
        d_lo = (r_our[0] - r_our[1]) - (r_ref[0] - r_ref[1])
        rows.append((p["name"], p[f"impact_{poi}"], d_lo, d_hi, q["fit"][1] - p["fit"][1]))

        # A reference row whose own crossing collapsed onto the best fit carries
        # a spurious zero impact, so it cannot be used as truth for that side.
        if p["fit"][0] == p["fit"][1] or p["fit"][2] == p["fit"][1]:
            degenerate.append(p["name"])
        elif not q.get("converged", True):
            unconverged.append(p["name"])
        else:
            worst = max(worst, abs(d_hi), abs(d_lo))

    print(f"{'nuisance':>34} {'impact':>11} {'d(impact-)':>12} {'d(impact+)':>12} {'d(theta)':>11}")
    for name, imp, d_lo, d_hi, d_th in rows:
        print(f"{name:>34} {imp:>11.6f} {d_lo:>12.2e} {d_hi:>12.2e} {d_th:>11.2e}")

    ref_poi = ref["POIs"][0]["fit"][1]
    our_poi = ours["POIs"][0]["fit"][1]
    good = len(rows) - len(degenerate) - len(unconverged)
    print(f"\nbest-fit {poi}: combine {ref_poi:.6f}  kestrel {our_poi:.6f}  "
          f"delta {our_poi - ref_poi:+.2e}")
    print(f"worst impact disagreement: {worst:.2e} over {good} comparable nuisances")
    if degenerate:
        print(f"\nCOMBINE DEGENERATE (its own crossing collapsed onto the best fit, so its "
              f"impact on that side is a spurious zero and cannot be compared against):"
              f"\n  {', '.join(degenerate)}")
    if unconverged:
        print(f"\nKESTREL FLAGGED converged=false, excluded from the statistic:"
              f"\n  {', '.join(unconverged)}")
    if missing:
        print(f"\nMISSING from kestrel: {', '.join(missing)}")
    extra = set(mine) - {p["name"] for p in ref["params"]}
    if extra:
        print(f"\nNOT IN THE REFERENCE (combine wrote too few entries to its own tree, so its "
              f"collection stage drops these too): {', '.join(sorted(extra))}")
    return 0 if worst < 1e-3 and not missing else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("card", type=pathlib.Path)
    ap.add_argument("--mass", default="120")
    ap.add_argument("--r-min", type=float, default=-5.0)
    ap.add_argument("--r-max", type=float, default=5.0)
    ap.add_argument("-o", "--output", default="impacts_reference.json")
    ap.add_argument("--check", action="store_true", help="run kestrel and diff against it")
    ap.add_argument("--jobs", type=int, default=8, help="concurrent combine jobs")
    ap.add_argument("--work", type=pathlib.Path,
                    help="keep the working directory here and reuse any fits already in it; "
                         "the per-nuisance stage is slow enough to be worth not repeating")
    a = ap.parse_args()

    card = a.card.resolve()
    if "autoMCStats" in card.read_text():
        print("WARNING: this card uses autoMCStats; see PRECISION.md item 12.", file=sys.stderr)

    with tempfile.TemporaryDirectory() as tmp:
        work = a.work.resolve() if a.work else pathlib.Path(tmp)
        work.mkdir(parents=True, exist_ok=True)
        if not (work / "ws.root").exists():
            build_workspace(card, a.mass, work)
        # kestrel names the nuisances to fit. A name Combine does not recognise
        # makes its job fail loudly, which is itself worth knowing.
        ours = run_kestrel(card, a.mass, a.r_min, a.r_max, work)
        names = [p["name"] for p in ours["params"]]
        ref = run_impacts(work, a.mass, a.r_min, a.r_max, names, jobs=a.jobs)
        pathlib.Path(a.output).write_text(json.dumps(ref, indent=2))
        print(f"wrote {a.output}", file=sys.stderr)
        return compare(ref, ours) if a.check else 0


if __name__ == "__main__":
    raise SystemExit(main())
