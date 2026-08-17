# Status — Stand 2026-08-17

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
| M2 vertikaler Pfad (Hotkey bis Einfügen) | **verifiziert 2026-07-29** — Harness-Lauf 6/8, siehe `docs/m2-evidence/` |
| **M3 Stabilisierung des Basispfads** | **teilweise verifiziert 2026-08-17** — siehe unten |
| M4 bis M10 | **offen** |

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

## M3 — Stabilisierung (2026-08-17)

Ziel: der Basispfad Hotkey → Aufnahme → Parakeet V3 → **genau ein** kontrollierter
Einfügeversuch → bei Unsicherheit Zwischenablage plus sichtbare Meldung.

### Was geändert wurde

| Bereich | Änderung |
|---|---|
| Defekte Live-Injektion | Braucht jetzt **zwei** Schalter: `stream_injection` **und** `experimental_enabled`. Ein alter Store mit `stream_injection: true` aktiviert sie nicht mehr allein. |
| Transkripte in Logs | Klartext nur noch in **Debug-Builds**. Im Release-Build loggen STREAMDIAG, der Segmenter, der Abschlusslog und die Refinement-Ablehnung ausschließlich Längen und Gate-Namen — unabhängig von `debug_mode`. |
| Einfügepfad | Neu `paste_guard.rs` + `paste_transcript_guarded()`: Zielfenster beim Stop erfassen, Fokus und Rechtelage vor dem Einfügen prüfen, **genau ein** Versuch, Verifikation der Zwischenablage durch Rücklesen, danach erneute Fokusprüfung. |
| Sichtbare Meldung | Neuer Overlay-Zustand `notice` — erscheint **auch bei `overlay_style: none`** und blendet nach 9 s aus. Zusätzlich ein Toast im Hauptfenster, falls es offen ist. |
| Einstellungen | Store auf sichere Baseline gesetzt: Parakeet V3, `stream_injection: false`, `debug_mode: false`, `log_level: info`. Sicherung unter `settings_store.json.vor-stabilisierung-2026-08-17.bak`. |

### Aktueller Verifikationsstand

| Prüfung | Ergebnis |
|---|---|
| `cargo test --lib` frisch ausgeführt | **verifiziert** — 198 passed, 0 failed (vorher 190; 8 neue) |
| `cargo build --release` frisch | **verifiziert** — Exit 0, 5 min 53 s, `sprechstift.exe` 43,7 MB vom 17.08.2026 19:01 |
| Frontend `npm run build` | **verifiziert** — Exit 0 |
| Kein Transkript-Klartext im Release-Binary | **verifiziert am Artefakt** — siehe unten |
| Native Windows-Abnahme (`scripts/m3-verify.ps1`) | **offen** — Skript existiert, ist aber noch nicht gelaufen |
| 100 aufeinanderfolgende Diktate | **offen** |

#### Beleg für die Log-Bereinigung

Nicht der Quelltext wurde geprüft, sondern das ausgelieferte Binary: die
Formatzeichenketten der Klartext-Zweige kommen darin nicht mehr vor, die
Längen-Varianten schon.

| Formatzeichenkette | in `sprechstift.exe` |
|---|---|
| `STREAMDIAG committed(len={})={:?}` | **nein** |
| `STREAMDIAG delta(len={})={:?}` | **nein** |
| `Transcription completed in {:?}: '{}'` | **nein** |
| `gate={:?} original={:?} candidate={:?}` | **nein** |
| `STREAMDIAG committed_len=` | ja |
| `Transcription completed in {:?} ({} chars)` | ja |
| `Text refinement rejected: stage=` | ja |

Die Toolchain war zu Beginn nicht lauffähig: `cargo` fehlte in beiden Shell-PATHs (liegt unter
`~\.cargo\bin`), und der CMake-Cache von `transcribe-cpp-sys` war auf den Ninja-Generator
festgeschrieben, während der Build den Visual-Studio-Generator anforderte. Beides ist behoben,
der Weg dorthin steht in `docs/BUILD-WINDOWS.md`.

## Nächste Schritte

Als Issues erfasst unter https://github.com/MrP42/sprechstift/issues

1. **#1** `scripts/m3-verify.ps1` real ausführen (Notepad, Browser, VS Code, Word) und Evidenz ablegen
2. **#2** Dauerlauf `-Scenario endurance -Runs 100`
3. **#4** Ursache der defekten Live-Injektion messen, statt weiter zu raten
4. **#5** Refinement-Stufe erst nach bestandener Abnahme optional aktivieren
5. **#7** Installer, SBOM, Third-Party-Notices

## Repository

| | |
|---|---|
| `origin` | `git@github.com:MrP42/sprechstift.git` — **privat** |
| `upstream` | `https://github.com/cjpais/Handy.git` — fremdes Fork-Original, **niemals dorthin pushen** |
| Arbeitsbranch | `feat/m3-stabilize-paste-path` |
