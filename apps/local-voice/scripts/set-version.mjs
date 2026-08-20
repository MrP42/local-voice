#!/usr/bin/env node
// Sets the app version in the three files that must never disagree:
//
//   package.json          — what pnpm reports
//   src-tauri/Cargo.toml  — what the binary is compiled as
//   src-tauri/tauri.conf.json — what the updater compares against, and what
//                               the release workflow derives its tag from
//
// A mismatch is not cosmetic: the updater decides "newer?" by comparing the
// running app's version to `latest.json`. If Cargo.toml lags behind
// tauri.conf.json, the freshly installed build still reports the old version
// and offers itself the same update forever.
//
//   node scripts/set-version.mjs 0.3.0
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Usage: node scripts/set-version.mjs <major.minor.patch>");
  process.exit(1);
}

const edits = [
  {
    file: "package.json",
    pattern: /("version":\s*")[^"]+(")/,
  },
  {
    file: "src-tauri/tauri.conf.json",
    pattern: /("version":\s*")[^"]+(")/,
  },
  {
    // Anchored to the [package] version, which is the first `version = ` line
    // in the file — dependency versions must stay untouched.
    file: "src-tauri/Cargo.toml",
    pattern: /^(version = ")[^"]+(")/m,
  },
];

for (const { file, pattern } of edits) {
  const path = join(root, file);
  const before = readFileSync(path, "utf8");
  // Test before replacing: an unchanged file is the normal case when re-running
  // with the version already set, and must not be mistaken for a missed field.
  if (!pattern.test(before)) {
    console.error(`No version field matched in ${file} — aborting.`);
    process.exit(1);
  }
  writeFileSync(path, before.replace(pattern, `$1${version}$2`));
  console.log(`${file} -> ${version}`);
}

// `app-v`, not `v`: bare `v*` tags belong to the Handy subtree this repo
// carries (its own versions ran to v0.9.4) — see AGENTS.md.
console.log(
  `\nNext: commit, then \`git tag app-v${version} && git push --follow-tags\` to trigger the release workflow.`,
);
