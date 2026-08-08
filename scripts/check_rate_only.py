#!/usr/bin/env python3
"""Compare kestrel's rate-only fit against real Combine.

Strips the shape systematics from a datacard, fits the result with Combine, and
checks that `kestrel fit --ignore-shapes` on the *original* card reproduces it.
Both sides are then fitting the same model, so agreement should be at the level
of the minimiser, not of any approximation.

Run inside the Combine environment:

    conda activate combine
    python3 scripts/check_rate_only.py <card.txt> [--mass 200] [--tol 1e-4]
"""

import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

import strip_shapes

REPO = pathlib.Path(__file__).resolve().parent.parent
KESTREL = REPO / "rust/kestrel/target/release/kestrel"

# "Best fit r: 0.668675  -0.4289/+0.4531  (68% CL)"
KESTREL_RE = re.compile(r"Best fit r:\s*([\d.eE+-]+)\s+([+-][\d.eE+-]+)/([+-][\d.eE+-]+)")


def read_limit_tree(path):
    """Combine prints only 3 decimals, which cannot resolve a 1e-4 tolerance.
    The output tree holds the same numbers at full precision: with
    `--algo singles` it has the best fit followed by the two interval ends."""
    import ROOT  # only available inside the Combine environment

    f = ROOT.TFile.Open(str(path))
    t = f.Get("limit")
    vals = []
    for e in t:
        vals.append(float(e.r))
    f.Close()
    if len(vals) < 3:
        raise RuntimeError(f"expected 3 entries in the limit tree, got {vals}")
    best, lo, hi = vals[0], vals[1], vals[2]
    return best, lo - best, hi - best


def run(cmd, cwd):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if p.returncode != 0:
        sys.exit(f"FAILED: {' '.join(map(str, cmd))}\n{p.stdout}\n{p.stderr}")
    return p.stdout + p.stderr


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("card", type=pathlib.Path)
    ap.add_argument("--mass", default="120")
    ap.add_argument("--tol", type=float, default=1e-4)
    ap.add_argument("--range", default="-5,5", help="POI range, as rmin,rmax")
    args = ap.parse_args()

    rmin, rmax = args.range.split(",")
    work = pathlib.Path(tempfile.mkdtemp(prefix="kestrel_gate_"))

    # Combine resolves the `shapes` file relative to the card, so the card and
    # the files it names have to sit together in the work directory.
    shutil.copy(args.card, work / args.card.name)
    for line in args.card.read_text().splitlines():
        f = line.split("#", 1)[0].split()
        # The filename slot takes wildcards too, as in
        # `all_histograms_fps4_$CHANNEL.root`, so match rather than test.
        for tok in f[1:]:
            if not tok.endswith(".root"):
                continue
            pattern = re.sub(r"\$[A-Z]+", "*", tok)
            for src in args.card.parent.glob(pattern):
                if not (work / src.name).exists():
                    shutil.copy(src, work / src.name)

    original = work / args.card.name
    stripped_text, dropped = strip_shapes.strip(original.read_text())
    stripped = work / "stripped.txt"
    stripped.write_text(stripped_text)
    print(f"stripped {len(dropped)} shape systematic lines")

    run(["text2workspace.py", "stripped.txt", "-o", "ws.root", "--mass", args.mass], work)

    # Combine's default tolerance of 0.1 leaves it measurably short of the
    # minimum, so the comparison is run converged-to-converged. The loose run is
    # kept only to show that any residual gap is convergence, not the model.
    results = {}
    for label, extra in [
        ("combine (default tol)", []),
        ("combine (tol 1e-4)", ["--cminDefaultMinimizerTolerance", "0.0001"]),
    ]:
        run(
            ["combine", "-M", "MultiDimFit", "ws.root", "--algo", "singles",
             "--setParameterRanges", f"r={rmin},{rmax}", "--mass", args.mass] + extra,
            work,
        )
        results[label] = read_limit_tree(work / f"higgsCombineTest.MultiDimFit.mH{args.mass}.root")
    c_val, c_lo, c_hi = results["combine (tol 1e-4)"]

    out = run(
        [str(KESTREL), "fit", str(original), "--ignore-shapes",
         "--mass", args.mass, "--r-min", rmin, "--r-max", rmax],
        work,
    )
    m = KESTREL_RE.search(out)
    if not m:
        sys.exit(f"could not parse kestrel output:\n{out}")
    k_val, k_lo, k_hi = (float(x) for x in m.groups())

    results["kestrel --ignore-shapes"] = (k_val, k_lo, k_hi)
    print(f"\n{'':<24} {'r':>12} {'lo':>12} {'hi':>12}")
    for label, (v, lo, hi) in results.items():
        print(f"{label:<24} {v:>12.6f} {lo:>12.6f} {hi:>12.6f}")
    diffs = [abs(k_val - c_val), abs(k_lo - c_lo), abs(k_hi - c_hi)]
    print(f"{'|diff|':<10} {diffs[0]:>12.2e} {diffs[1]:>12.2e} {diffs[2]:>12.2e}")

    worst = max(diffs)
    print(f"\nworst = {worst:.3e}, tolerance = {args.tol:.1e}")
    print("GATE: PASS" if worst < args.tol else "GATE: FAIL")
    return 0 if worst < args.tol else 1


if __name__ == "__main__":
    raise SystemExit(main())
