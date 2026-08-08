#!/usr/bin/env python3
"""Delete shape systematics from a Combine datacard.

The result is the exact model kestrel builds in rate-only mode, so running real
Combine on it gives a reference our `--ignore-shapes` fit must reproduce.

    python3 scripts/strip_shapes.py card.txt > stripped.txt

Only `shape`, `shapeN`, `shapeU` and `shape?` *systematic* lines are removed.
The `shapes` (plural) lines that map processes onto histograms are kept, since
the nominal templates are still needed.
"""

import argparse
import sys

SHAPE_TYPES = {"shape", "shapeN", "shapeU", "shape?", "shapeN2", "shape2"}


def is_shape_systematic(line: str) -> bool:
    body = line.split("#", 1)[0].split()
    # `<name> <type> ...`; `shapes` (plural) is a mapping line, not a systematic.
    return len(body) >= 2 and body[0] != "shapes" and body[1] in SHAPE_TYPES


def strip(text: str) -> tuple[str, list[str]]:
    kept, dropped = [], []
    for line in text.splitlines():
        if is_shape_systematic(line):
            dropped.append(line.split()[0])
        else:
            kept.append(line)

    # kmax must still be consistent; `*` already means "infer".
    out = []
    for line in kept:
        f = line.split()
        if f and f[0] == "kmax" and len(f) > 1 and f[1] != "*":
            line = line.replace(f[1], "*", 1)
        out.append(line)
    return "\n".join(out) + "\n", dropped


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("card")
    ap.add_argument("-o", "--output", help="write here instead of stdout")
    args = ap.parse_args()

    with open(args.card) as fh:
        text = fh.read()
    stripped, dropped = strip(text)

    if args.output:
        with open(args.output, "w") as fh:
            fh.write(stripped)
    else:
        sys.stdout.write(stripped)

    uniq = sorted(set(dropped))
    print(
        f"removed {len(dropped)} shape systematic lines ({len(uniq)} distinct)",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
