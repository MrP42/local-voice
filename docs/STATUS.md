# Status — Stand 2026-07-28

Nur real ausgeführte und verifizierte Dinge stehen unter „verifiziert".

## Phase 2 — Skill `research-first-rebuilder`

| Punkt | Status |
|---|---|
| Skill-Quelle (`tooling/research-first-rebuilder/`) | **fertig** — SKILL.md, 4 Referenzen, 12 Templates, 3 Skripte |
| Hilfsskripte real getestet | **verifiziert** — `repo_health.py` gegen echtes und nicht existierendes Repo; `license_scan.py` gegen 5 konstruierte Lizenzfälle (Stock-Apache, Apache+Commons-Clause, Stub, Brand-Carve-out, keine Lizenz) |
| Dabei gefundene und behobene Fehler | 2 Windows-Encoding-Bugs (cp1252) + ein Apache-2.0-Falschalarm |
| Installation | **verifiziert** — 23 Dateien unter `~/.claude/skills/research-first-rebuilder/`, Dev-Evals korrekt ausgeschlossen |
| Eval-Läufe | **teilweise** — 4 Testfälle x (mit/ohne Skill) gestartet; 2 Paare vollständig, 2 Paare unvollständig |
| Grading | **teilweise** — programmatischer Grader (`grade.py`) geschrieben und ausgeführt |
| Benchmark-Aggregation, Eval-Viewer, Description-Optimierung | **offen** |

### Bisherige Eval-Ergebnisse (real ausgeführt)

| Testfall | mit Skill | ohne Skill |
|---|---|---|
| `not-a-rebuild-simple-bugfix` (Nicht-Trigger) | **5/5** | 5/5 |
| `refuse-asset-theft-but-help` | **6/6** | 6/6 |
| `local-dictation-alternative` | unvollständig | unvollständig |
| `dont-reinvent-find-fork` | unvollständig | unvollständig |

**Ehrliche Einordnung:** Der Nicht-Trigger-Fall bestätigt, dass der Skill bei einem simplen
Bugfix **nicht** den 10-Gate-Workflow auslöst — das war die wichtigste negative Anforderung.
Beim Ablehnungsfall ist **kein** Vorteil gegenüber der Baseline messbar; ein zunächst
scheinbarer Vorteil war ein Fehler in meinem Grader-Regex und wurde korrigiert, statt ihn
stehen zu lassen. Ein belastbarer Gesamtvergleich Skill gegen Baseline liegt **noch nicht** vor.

## Phase 3 — Anwendung „Sprechstift"

| Meilenstein | Status |
|---|---|
| M-1 Rust-Toolchain | **verifiziert** — rustc/cargo 1.97.1, stable-x86_64-pc-windows-msvc |
| M0.1 Lizenzen | **verifiziert** — `cargo deny check licenses` ok; kein GPL/LGPL/AGPL, keine unlizenzierte Crate auf dem Windows-Zielgraphen |
| M0.2 Advisories | **verifiziert** — 8 Vulnerabilities gefunden, 6 behoben, 2 dokumentiert ignoriert |
| M0.3 Namensprüfung | **verifiziert** — Sprechstift frei auf npm, crates.io, PyPI, GitHub |
| M0.5 Baseline-Build | **verifiziert** — nach Behebung des Vulkan-SDK-Blockers |
| M0.6 Modell-Integrität | **verifiziert** — Parakeet V3 SHA-256 stimmt mit dem Katalogwert überein |
| M1 Fork + Rebranding | **verifiziert** — Subtree-Import mit voller Historie, Updater und SignCommand entfernt, App startet unter eigenem Namen |
| **Lokale deutsche Transkription** | **verifiziert** — siehe Messung unten |
| M2 vollständiger vertikaler Pfad (Hotkey bis Einfügen) | **offen** — Transkription verifiziert, Hotkey und Injektion noch nicht real durchlaufen |
| M3 bis M10 | **offen** |

### Verifizierte Messung (2026-07-28, i9-13900K, CPU-Backend)

```
Modell     : Parakeet TDT 0.6B v3, int8, CC-BY-4.0 (Attribution: NVIDIA)
Audio      : 9,15 s deutsch
Modellladen: 1035 ms
Inferenz   : 392 ms  (etwa 23x Echtzeit, ohne GPU)
Ausgabe    : "Guten Tag, dies ist ein Test der lokalen Spracherkennung.
              Der Termin ist am 3. Februar um 14.30 Uhr."
```

Korrekte deutsche Groß- und Kleinschreibung sowie Interpunktion; gesprochene Zahlen normalisiert
(dritten Februar wird zu 3. Februar, vierzehn Uhr dreißig wird zu 14.30 Uhr).

## Nächste Schritte

1. M2 abschließen: Hotkey, Aufnahme, Transkription, Einfügen real in Notepad durchlaufen
2. Eval-Läufe für die zwei Recherche-Testfälle abschließen, danach Benchmark und Description-Optimierung
3. Regelbasierte Nachbearbeitung und Texttreue-Validator (Gates 0 bis 5)
4. Injektions-Fallback-Kette und App-Matrix
5. Installer, SBOM, Third-Party-Notices
