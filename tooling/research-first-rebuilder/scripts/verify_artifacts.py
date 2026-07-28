#!/usr/bin/env python3
"""Check that the artifacts a given gate is supposed to produce actually exist.

The point is to catch the failure mode where a gate is declared "done" but left no
trace on disk. It checks presence and non-emptiness, not correctness.

Usage:
    python verify_artifacts.py <output-directory> [--gate N] [--json]
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# gate -> (label, [relative paths, any-of within a group separated by "|"])
GATES: dict[int, tuple[str, list[str]]] = {
    1: ("Ziel und Constraints", ["docs/research/00-auftrag.md"]),
    2: ("Öffentliche Produktrecherche", [
        "docs/research/01-produktanalyse.md",
        "docs/research/02-quellenverzeichnis.md",
        "docs/research/03-feature-paritaet.md",
    ]),
    3: ("OSS- und Komponentenrecherche", ["docs/research/04-oss-reuse-matrix.md"]),
    4: ("Lizenz- und Sicherheitsprüfung", ["docs/research/05-lizenzpruefung.md"]),
    5: ("Fork/Reuse/Build-Entscheidung", [
        "docs/research/06-architekturentscheidung.md",
        "docs/DECISIONS.md",
    ]),
    6: ("Umsetzungsplan", [
        "docs/research/07-implementierungsplan.md",
        "docs/research/08-threat-model.md",
    ]),
    7: ("Vertikaler MVP", ["docs/STATUS.md"]),
    8: ("Funktionsumfang erweitern", ["docs/STATUS.md"]),
    9: ("Reale Ausführung", ["docs/research/10-akzeptanzkriterien.md"]),
    10: ("Packaging und Abschluss", [
        "docs/TRACEABILITY.md",
        "docs/KNOWN-LIMITATIONS.md",
        "docs/research/11-abschlussbericht.md",
        "THIRD-PARTY-NOTICES.md|THIRD_PARTY_NOTICES.md|docs/THIRD-PARTY-NOTICES.md",
    ]),
}

MIN_BYTES = 200  # a template with nothing filled in is not an artifact

# Windows consoles default to a legacy code page; reconfigure so em dashes survive.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def check_one(root: Path, spec: str) -> dict:
    for candidate in spec.split("|"):
        p = root / candidate
        if p.is_file():
            size = p.stat().st_size
            return {
                "spec": spec,
                "found": candidate,
                "bytes": size,
                "ok": size >= MIN_BYTES,
                "note": "" if size >= MIN_BYTES else f"present but nearly empty (<{MIN_BYTES}B)",
            }
    return {"spec": spec, "found": None, "bytes": 0, "ok": False, "note": "missing"}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("output_directory", type=Path)
    ap.add_argument("--gate", type=int, choices=sorted(GATES), help="check one gate (default: all)")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    root = args.output_directory.resolve()
    if not root.is_dir():
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    gates = [args.gate] if args.gate else sorted(GATES)
    report, all_ok = [], True
    for g in gates:
        label, specs = GATES[g]
        checks = [check_one(root, s) for s in specs]
        ok = all(c["ok"] for c in checks)
        all_ok &= ok
        report.append({"gate": g, "label": label, "ok": ok, "checks": checks})

    if args.json:
        print(json.dumps({"root": str(root), "all_ok": all_ok, "gates": report}, indent=2))
        return 0 if all_ok else 1

    for entry in report:
        mark = "PASS" if entry["ok"] else "FAIL"
        print(f"[{mark}] Gate {entry['gate']} — {entry['label']}")
        for c in entry["checks"]:
            if c["ok"]:
                print(f"        ok  {c['found']} ({c['bytes']}B)")
            else:
                print(f"        !!  {c['spec']}: {c['note']}")
    print("\nPresence only — this does not verify content quality.")
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
