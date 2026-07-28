# Entscheidungen

Evidenzklassen: **OBSERVED** (selbst geprüft) · **SOURCE-CLAIM** · **INFERRED** · **UNKNOWN**

## D1 — Fork von `cjpais/Handy` statt Eigenentwicklung
**Datum:** 2026-07-28 · **Status:** entschieden

Nach der gewichteten Bewertung (funktionale Passung 25 %, Lizenz 20 %, Plattform/HW 15 %,
Wartung 15 %, Codequalität 10 %, Datenschutz 10 %, Erweiterbarkeit 5 %) erreichte Handy als
einziger Kandidat gleichzeitig: vollständiger Hotkey→Cursor-Pfad, verifiziertes MIT, aktive
Multi-Contributor-Wartung, keine Telemetrie, kein Konto, deutsche UI bereits vorhanden.

**Lizenz OBSERVED:** LICENSE-Blob `ff8dfab0159b41263ccc3c50da54007ca6752a22` am Commit
`ea3c20a3a67c7401d8b19198723760da9d40ac45` direkt aus dem Repository gelesen — Stock-MIT-Text,
© 2025 CJ Pais, keine Zusatzklauseln.

Ausgeschlossen wurden u. a. `whisper-writer` und `typewhisper-win` (GPL-3.0), `dictto`,
`voicetypr`, `voquill` (AGPL bzw. Lizenz-Stub ohne Volltext). `blurredmachine/VoiceTyping`
existiert nicht (GitHub 404) — OBSERVED.

## D2 — Rebranding ist Pflicht, nicht Kosmetik
**OBSERVED**, Handy-README am Fork-Commit, wörtlich:
> "Handy is open-source software, but the Handy name, logo, icon, and brand assets are not
> open-source. Unofficial forks, rewrites, and redistributions must use their own branding."

Die MIT-Lizenz deckt den Code, nicht die Kennzeichen. Produktname **Sprechstift**, Identifier
`de.wolffappliedai.sprechstift`. Namensprüfung OBSERVED: npm, crates.io, PyPI je HTTP 404,
GitHub-Repo-Suche 0 Treffer (2026-07-28).

## D3 — Kein Claude-Code-CLI- und kein Codex-CLI-Provider
**Status:** entschieden, nicht implementiert

Der Auftrag erlaubt solche Provider ausdrücklich nur, wenn die geltende offizielle Dokumentation
den Einsatz stützt. Sie tut es nicht.

**Anthropic — ausdrücklich untersagt.** `https://code.claude.com/docs/en/legal-and-compliance`
(abgerufen 2026-07-28), wörtlich:
> "Anthropic does not permit third-party developers to offer Claude.ai login or to route requests
> through Free, Pro, or Max plan credentials on behalf of their users."

Ergänzend Consumer Terms §3: Zugriff "through automated or non-human means, whether through a
bot, script, or otherwise" ist außerhalb von API-Keys untersagt. Eine App, die `claude -p`
startet, ist ein solches Skript.

**OpenAI — nicht verifizierbar.** Die Terms of Use lieferten bei drei URLs HTTP 403; das
Web-Archiv war blockiert. Die geforderte Bedingung ist damit nicht belegbar erfüllt.

**Kosten dieser Entscheidung: keine.** Der generische `OpenAiCompatProcessor` erreicht Anthropic
und OpenAI bereits auf dem ausdrücklich gesegneten Weg — per nutzereigenem API-Key mit Base-URL —
über denselben Codepfad wie Ollama.

⚠️ Anthropics Politik änderte sich 2026 etwa quartalsweise. Vor einem Release neu prüfen.

## D4 — Vulkan hinter ein Cargo-Feature statt hart verdrahtet
**OBSERVED:** Upstream setzt für Windows x86_64 fest `features = ["dynamic-backends","vulkan"]`.
Ohne LunarG Vulkan SDK bricht CMake ab: `Could NOT find Vulkan (missing: Vulkan_LIBRARY
Vulkan_INCLUDE_DIR glslc)`. Der NVIDIA-Laufzeittreiber allein genügt nicht.

Geändert zu Opt-in `--features gpu-vulkan`. Ein frischer Checkout baut damit CPU-only.

**Gemessen (OBSERVED, 2026-07-28, i9-13900K, CPU-Backend `ggml-cpu-alderlake`):**
Parakeet V3 int8, 9,15 s deutsches Audio → Modell laden 1035 ms, Inferenz **392 ms** (≈23×
Echtzeit). Die CPU-Leistung ist für Diktatlängen mehr als ausreichend; die Installation des
Vulkan-SDK ist damit **keine Voraussetzung**, sondern eine Optimierung.

## D5 — Updater und Code-Signatur des Upstream entfernt
Der Upstream-Updater zeigte auf `https://github.com/cjpais/Handy/releases/latest/download/latest.json`
(OBSERVED). Ein Fork darf diese Artefakte nicht laden. Plugin, Abhängigkeit, Capabilities und
Konfiguration wurden entfernt — das beseitigt zugleich einen ausgehenden Netzwerkpfad.
`bundle.windows.signCommand` verwies auf das Azure-Trusted-Signing-Konto von CJ Pais und wurde
entfernt.

## D6 — Sicherheitslage der Abhängigkeiten
**OBSERVED, `cargo audit` 2026-07-28:** 8 Vulnerabilities im geforkten Stand.
Behoben durch Update: `rustls-webpki` 0.103.9→0.103.13 (4 Advisories, u. a. fehlerhafte
CRL-/Name-Constraint-Prüfung) und `tar` 0.4.44→0.4.46 (2 Advisories, u. a. `unpack_in` folgt
Symlinks beim chmod). Beide liegen direkt auf unserem Modell-Download-Pfad.

Verbleibend: `quick-xml` RUSTSEC-2026-0194/0195 (DoS-Klasse), erreichbar nur über `plist`
(macOS-Info.plist bzw. Build-Zeit-Codegen). `plist 1.8.0` pinnt `quick-xml "^0.38"`, die
gepatchte 0.41 ist ohne Upstream-Release nicht wählbar. In `deny.toml` dokumentiert ignoriert.

**Lizenzen OBSERVED:** Auf dem Windows-Zielgraphen kein GPL/LGPL/AGPL und keine Crate ohne
Lizenzfeld. MPL-2.0 (17 Crates, u. a. symphonia) ist dateibasiertes schwaches Copyleft und
verträglich. `cargo deny check licenses` → **ok**.
