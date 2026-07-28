# Upstream

This application is a fork of **Handy** by CJ Pais.

| | |
|---|---|
| Upstream repository | https://github.com/cjpais/Handy |
| Upstream commit | `ea3c20a3a67c7401d8b19198723760da9d40ac45` |
| Commit date | 2026-07-28 |
| Upstream version at fork point | 0.9.4 |
| Licence | MIT (LICENSE blob `ff8dfab0159b41263ccc3c50da54007ca6752a22`, Copyright (c) 2025 CJ Pais) |
| Import method | `git subtree add --prefix=apps/local-voice upstream ea3c20a3` — full upstream history preserved |

The LICENSE file was read directly from the repository at that exact commit, not from a
search result or the GitHub sidebar label.

## Branding — why the rename was mandatory

Handy's README states verbatim:

> Handy is open-source software, but the Handy name, logo, icon, and brand assets are not
> open-source. Unofficial forks, rewrites, and redistributions must use their own branding and
> must not imply endorsement or affiliation.

The MIT licence therefore covers the code but **not** the marks. This fork ships under its own
name (Sprechstift), its own bundle identifier and its own branding, and claims no endorsement by
or affiliation with CJ Pais or the Handy project.

## Deliberate deviations from upstream

| Change | Reason |
|---|---|
| `productName` → `Sprechstift`, identifier → `de.wolffappliedai.sprechstift` | Mandatory rebranding (above) |
| Version reset to `0.1.0` | This fork's own versioning; not a continuation of 0.9.4 |
| Cargo package `handy` → `sprechstift`, lib `handy_app_lib` → `sprechstift_app_lib` | Consistent identity |
| **Vulkan moved behind a `gpu-vulkan` cargo feature** | Upstream hardcodes `vulkan` for Windows x86_64, which fails to configure without the LunarG Vulkan SDK. A fresh checkout now builds CPU-only; GPU is a documented opt-in. |
| **`tauri-plugin-updater` removed entirely** (plugin, dependency, capabilities, config) | Upstream's updater points at `cjpais/Handy` releases. A fork must never pull those artifacts, and this app ships no update server. Also removes an outbound network path. |
| **`bundle.windows.signCommand` removed** | Upstream signs with CJ Pais' Azure Trusted Signing account, which is not ours to use. |
| `beforeDevCommand` / `beforeBuildCommand` → `pnpm` | `bun` is not part of this toolchain. |
| `postinstall` script removed | It was a bun-only Nix dependency check, irrelevant on Windows. |
| Vulnerable transitive deps updated (`rustls-webpki` 0.103.9→0.103.13, `tar` 0.4.44→0.4.46) | 6 RUSTSEC advisories fixed; see `docs/DECISIONS.md`. |
| `deny.toml` added | Codifies the licence and advisory policy verified in M0. |

Upstream changes can still be pulled with:

```
git subtree pull --prefix=apps/local-voice upstream main
```
