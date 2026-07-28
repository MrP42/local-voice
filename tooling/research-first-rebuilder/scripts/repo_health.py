#!/usr/bin/env python3
"""Summarize a GitHub repository's health for the Gate 3 candidate matrix.

Uses the `gh` CLI when available (so it inherits the user's auth and rate limit),
otherwise falls back to unauthenticated REST.

Usage:
    python repo_health.py owner/name [owner/name ...] [--json]
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import shutil
import subprocess
import sys
import urllib.request

API = "https://api.github.com"

# Windows consoles default to a legacy code page; without this, printing an em dash
# raises UnicodeEncodeError or emits mojibake.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


def _via_gh(path: str):
    if not shutil.which("gh"):
        return None
    try:
        # encoding must be explicit: on Windows `text=True` decodes with the ANSI code
        # page (cp1252) and blows up on UTF-8 payloads from the GitHub API.
        out = subprocess.run(
            ["gh", "api", path],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=45,
            check=False,
        )
        if out.returncode == 0:
            return json.loads(out.stdout)
    except (subprocess.SubprocessError, json.JSONDecodeError):
        pass
    return None


def api(path: str):
    got = _via_gh(path)
    if got is not None:
        return got
    req = urllib.request.Request(
        f"{API}{path}", headers={"Accept": "application/vnd.github+json", "User-Agent": "repo-health"}
    )
    try:
        with urllib.request.urlopen(req, timeout=45) as r:
            return json.loads(r.read())
    except Exception as exc:  # noqa: BLE001 - surfacing the reason matters more than the type
        return {"_error": str(exc)}


def days_since(iso: str | None) -> int | None:
    if not iso:
        return None
    when = dt.datetime.fromisoformat(iso.replace("Z", "+00:00"))
    return (dt.datetime.now(dt.timezone.utc) - when).days


def inspect(repo: str) -> dict:
    meta = api(f"/repos/{repo}")
    if "_error" in meta or meta.get("message") == "Not Found":
        return {
            "repo": repo,
            "exists": False,
            "note": "REPOSITORY NOT FOUND — report this plainly, do not paraphrase it into existence",
            "detail": meta.get("_error") or meta.get("message"),
        }

    commits = api(f"/repos/{repo}/commits?per_page=100")
    committers, last_date = set(), None
    if isinstance(commits, list) and commits:
        last_date = commits[0].get("commit", {}).get("committer", {}).get("date")
        for c in commits:
            author = c.get("author") or {}
            login = author.get("login") or c.get("commit", {}).get("author", {}).get("name")
            if login:
                committers.add(login)

    releases = api(f"/repos/{repo}/releases?per_page=1")
    latest = releases[0] if isinstance(releases, list) and releases else {}

    return {
        "repo": repo,
        "exists": True,
        "license_label": (meta.get("license") or {}).get("spdx_id"),
        "license_warning": "sidebar label is heuristic — read the LICENSE blob at the commit",
        "stars": meta.get("stargazers_count"),
        "forks": meta.get("forks_count"),
        "open_issues": meta.get("open_issues_count"),
        "archived": meta.get("archived"),
        "default_branch": meta.get("default_branch"),
        "language": meta.get("language"),
        "pushed_at": meta.get("pushed_at"),
        "days_since_push": days_since(meta.get("pushed_at")),
        "last_commit": last_date,
        "days_since_commit": days_since(last_date),
        "distinct_recent_committers": len(committers),
        "latest_release": latest.get("tag_name"),
        "latest_release_at": latest.get("published_at"),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("repos", nargs="+", metavar="owner/name")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    results = [inspect(r) for r in args.repos]

    if args.json:
        print(json.dumps(results, indent=2))
        return 0

    for r in results:
        print(f"\n=== {r['repo']} ===")
        if not r["exists"]:
            print(f"  !! {r['note']}")
            print(f"     ({r.get('detail')})")
            continue
        arch = "  [ARCHIVED]" if r.get("archived") else ""
        print(f"  {r.get('language')}  stars={r.get('stars')}  forks={r.get('forks')}  "
              f"open_issues={r.get('open_issues')}{arch}")
        print(f"  license label : {r.get('license_label')}  <- {r['license_warning']}")
        print(f"  last commit   : {r.get('last_commit')} ({r.get('days_since_commit')} days ago)")
        print(f"  recent committers (last 100 commits): {r.get('distinct_recent_committers')}")
        print(f"  latest release: {r.get('latest_release')} ({r.get('latest_release_at')})")
        stale = r.get("days_since_commit")
        if stale is not None and stale > 365:
            print("  !! No commit in over a year — treat as unmaintained unless proven otherwise")
        if r.get("distinct_recent_committers") == 1:
            print("  !! Single-committer project — bus factor 1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
