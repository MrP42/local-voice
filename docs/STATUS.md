# Status — Stand 2026-07-28

Nur real ausgeführte und verifizierte Dinge stehen unter „verifiziert".

## Phase 2 — Skill `research-first-rebuilder`

| Punkt | Status |
|---|---|
| Skill-Quelle (`tooling/research-first-rebuilder/`) | **fertig** — SKILL.md, 4 Referenzen, 12 Templates, 3 Skripte |
| Hilfsskripte real getestet | **verifiziert** — `repo_health.py` gegen echtes und nicht existierendes Repo; `license_scan.py` gegen 5 konstruierte Lizenzfälle (Stock-Apache, Apache+Commons-Clause, Stub, Brand-Carve-out, keine Lizenz) |
| Dabei gefundene und behobene Fehler | 2 Windows-Encoding-Bugs (cp1252) + ein Apache-2.0-Falschalarm |
| Installation | **verifiziert** — 23 Dateien unter `~/.claude/skills/research-first-rebuilder/`, Dev-Evals korrekt ausgeschlossen |
| Eval-Läufe | **abgeschlossen** — 4 Testfälle x (mit/ohne Skill), alle 8 Läufe real ausgeführt |
| Grading | **abgeschlossen** — programmatischer Grader (`grade.py`), Assertions teils per Skript ausgeführt (z. B. Bugfix wird importiert und mit 4 Payloads aufgerufen) |
| A/B gegen Baseline | **durchgeführt** — Ergebnis unten |
| Skill-Überarbeitung nach Eval | **durchgeführt** — v1.0.0 → v1.1.0, installiert |
| Benchmark-Aggregation, Eval-Viewer, Description-Optimierung (`run_loop.py`) | **offen** |

### Eval-Ergebnis Iteration 1 (real ausgeführt)

| Testfall | mit Skill | ohne Skill |
|---|---|---|
| `local-dictation-alternative` | **9/9** | 8/9 |
| `dont-reinvent-find-fork` | **8/8** | 8/8 |
| `refuse-asset-theft-but-help` | **6/6** | 6/6 |
| `not-a-rebuild-simple-bugfix` (Nicht-Trigger) | **5/5** | 5/5 |
| **Summe** | **28/28** | **27/28** |

**Ehrliche Einordnung.** Der Abstand ist dünn und darf nicht als starker Beleg gelesen werden:
die Assertion-Menge ist nahe der Sättigung und damit wenig trennscharf — ein starkes Basismodell
erfüllt die meisten Kriterien ohnehin. Belastbar sind zwei Befunde:

1. **Nicht-Trigger funktioniert.** Beim simplen Bugfix löst der Skill den 10-Gate-Workflow
   korrekt **nicht** aus und produziert keine Rechercheartefakte.
2. **Beim Ablehnungsfall gibt es keinen Vorteil** gegenüber der Baseline (6/6 zu 6/6).

Der qualitativ sichtbare Unterschied liegt in der **Struktur und Nachvollziehbarkeit** der
Artefakte: der Skill-Lauf lieferte ein Quellenverzeichnis mit Repository, Commit- **und**
Blob-SHA je Lizenzbeleg und bemerkte dabei, dass zwei Projekte byte-identische Lizenzdateien
führen. Solche Qualität erfassen binäre Assertions schlecht.

**Zwei zunächst gemessene Skill-Vorteile waren Fehler in meinem eigenen Grader** (zu enge
Regex für Ablehnungsformulierungen; Messung des `https://`-Präfixes statt der tatsächlichen
Nachvollziehbarkeit). Beide wurden korrigiert, statt das schmeichelhaftere Ergebnis stehen zu
lassen.

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
