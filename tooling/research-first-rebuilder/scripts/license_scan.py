#!/usr/bin/env python3
"""Find and classify license files in a checkout, and flag the traps that matter.

This is ZUARBEIT, not a decision. It tells you where to look and what smells wrong.
Always read the actual LICENSE file yourself before deciding anything.

Usage:
    python license_scan.py <path> [--json]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

LICENSE_NAMES = re.compile(
    r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|LICENCE)([-_.].*)?(\.(md|txt|rst))?$",
    re.IGNORECASE,
)

# Ordered: first match wins, so put more specific patterns first.
SIGNATURES: list[tuple[str, re.Pattern[str]]] = [
    ("AGPL-3.0", re.compile(r"GNU AFFERO GENERAL PUBLIC LICENSE", re.I)),
    ("GPL-3.0", re.compile(r"GNU GENERAL PUBLIC LICENSE\s+Version 3", re.I)),
    ("GPL-2.0", re.compile(r"GNU GENERAL PUBLIC LICENSE\s+Version 2", re.I)),
    ("LGPL", re.compile(r"GNU LESSER GENERAL PUBLIC LICENSE", re.I)),
    ("MPL-2.0", re.compile(r"Mozilla Public License Version 2\.0", re.I)),
    ("EPL", re.compile(r"Eclipse Public License", re.I)),
    ("Apache-2.0", re.compile(r"Apache License\s+Version 2\.0", re.I)),
    ("BSL-1.0", re.compile(r"Boost Software License", re.I)),
    ("BUSL", re.compile(r"Business Source License", re.I)),
    ("SSPL", re.compile(r"Server Side Public License", re.I)),
    ("Elastic", re.compile(r"Elastic License", re.I)),
    ("CC0", re.compile(r"CC0 1\.0|Creative Commons Zero", re.I)),
    ("CC-BY-NC", re.compile(r"Attribution-NonCommercial", re.I)),
    ("CC-BY-SA", re.compile(r"Attribution-ShareAlike", re.I)),
    ("CC-BY-4.0", re.compile(r"Creative Commons Attribution 4\.0", re.I)),
    ("Unlicense", re.compile(r"This is free and unencumbered software", re.I)),
    ("ISC", re.compile(r"ISC License", re.I)),
    ("BSD-3-Clause", re.compile(r"Neither the name of", re.I)),
    ("BSD-2-Clause", re.compile(r"Redistributions in binary form must reproduce", re.I)),
    ("MIT", re.compile(r"Permission is hereby granted, free of charge", re.I)),
]

COPYLEFT = {
    "AGPL-3.0": "network-copyleft",
    "GPL-3.0": "strong-copyleft",
    "GPL-2.0": "strong-copyleft",
    "LGPL": "weak-copyleft",
    "MPL-2.0": "weak-copyleft",
    "EPL": "weak-copyleft",
    "BUSL": "source-available-restricted",
    "SSPL": "source-available-restricted",
    "Elastic": "source-available-restricted",
    "CC-BY-NC": "non-commercial",
}

# Windows consoles default to a legacy code page; reconfigure so em dashes survive.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

COPYRIGHT = re.compile(r"(Copyright|\(c\)|©)\s*(\(c\)\s*)?\d{4}", re.I)
OPEN_CORE_DIRS = {"enterprise", "ee", "pro", "commercial", "paid"}
SKIP_DIRS = {".git", "node_modules", "target", "dist", "build", "__pycache__", ".venv"}

# Stock Apache-2.0 legitimately contains the words "additional terms or conditions"
# (in sections 4 and 5). Strip those known-good sentences before hunting for real
# add-on clauses, otherwise every Apache project trips the flag and the warning
# becomes noise that people learn to ignore.
STOCK_PHRASES = re.compile(
    r"without any additional terms or conditions"
    r"|may provide additional or different license terms and conditions"
    r"|You may add Your own copyright statement",
    re.I,
)

# Phrases that mean "this is not stock OSI Apache/MIT any more".
EXTRA_TERMS = re.compile(
    r"additional (terms|conditions|restrictions)"
    r"|subject to the following additional"
    r"|Commons Clause"
    r"|may not be used to compete"
    r"|shall not be used to provide a (commercial |managed |hosted )?service",
    re.I,
)

# Licenses that normally name the rights holder inline. Apache-2.0 and the GNU
# family carry only a bracketed placeholder, so a missing name there is expected
# and flagging it would be a false alarm.
HOLDER_EXPECTED_INLINE = {"MIT", "ISC", "BSD-2-Clause", "BSD-3-Clause", "BSL-1.0"}
BRAND_CARVEOUT = re.compile(
    r"(name|logo|icon|trademark|brand)s?[^.\n]{0,80}(are|is) not (covered|licensed|included)"
    r"|except for the (trademarks|logos|name)"
    r"|trademarks?[^.\n]{0,40}not granted",
    re.I,
)


def classify(text: str) -> list[str]:
    return [name for name, pat in SIGNATURES if pat.search(text)]


def scan_file(path: Path, root: Path) -> dict:
    try:
        raw = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:  # unreadable file is itself a finding
        return {"path": str(path.relative_to(root)), "error": str(exc)}

    matches = classify(raw)
    size = len(raw.encode("utf-8"))
    primary = matches[0] if matches else None
    flags: list[str] = []

    # A real license text is long. A 140-byte file naming AGPL is not a license.
    if size < 400 and path.name.upper() not in {"NOTICE"}:
        flags.append("STUB: file too short to contain an operative license text")
    if not matches:
        flags.append("UNRECOGNIZED: no known license signature matched — read it manually")
    if len(matches) > 1:
        flags.append(f"MULTIPLE signatures matched ({', '.join(matches)}) — dual-license or vendored?")
    if primary in HOLDER_EXPECTED_INLINE and not COPYRIGHT.search(raw):
        flags.append("NO COPYRIGHT LINE: this license normally names a rights holder inline")
    if EXTRA_TERMS.search(STOCK_PHRASES.sub("", raw)):
        flags.append("EXTRA TERMS: additional conditions present — NOT stock OSI terms")
    if BRAND_CARVEOUT.search(raw):
        flags.append("BRAND CARVE-OUT: name/logo/icons appear excluded — branding must be replaced")

    return {
        "path": str(path.relative_to(root)).replace("\\", "/"),
        "bytes": size,
        "license": primary,
        "all_matches": matches,
        "copyleft": COPYLEFT.get(primary or "", "permissive" if primary else "unknown"),
        "flags": flags,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("path", type=Path)
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    args = ap.parse_args()

    root = args.path.resolve()
    if not root.is_dir():
        print(f"error: not a directory: {root}", file=sys.stderr)
        return 2

    findings, open_core = [], []
    for p in sorted(root.rglob("*")):
        if any(part in SKIP_DIRS for part in p.parts):
            continue
        if p.is_dir():
            if p.name.lower() in OPEN_CORE_DIRS:
                open_core.append(str(p.relative_to(root)).replace("\\", "/"))
        elif p.is_file() and LICENSE_NAMES.match(p.name):
            findings.append(scan_file(p, root))

    result = {
        "root": str(root),
        "license_files": findings,
        "possible_open_core_dirs": sorted(open_core),
        "reminder": "Scripts do not decide licenses. Read the file at the exact commit yourself.",
    }

    if args.json:
        print(json.dumps(result, indent=2))
        return 0

    if not findings:
        print("!! NO LICENSE FILE FOUND — treat as 'all rights reserved'. Do not copy or fork.")
    for f in findings:
        if "error" in f:
            print(f"  {f['path']}: UNREADABLE ({f['error']})")
            continue
        print(f"\n{f['path']}  [{f['bytes']} bytes]")
        print(f"  license : {f['license'] or 'UNRECOGNIZED'}  ({f['copyleft']})")
        for flag in f["flags"]:
            print(f"  !! {flag}")
    if open_core:
        print("\n!! Possible open-core subtrees (may carry their own proprietary license):")
        for d in open_core:
            print(f"   - {d}")
    print(f"\n{result['reminder']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
