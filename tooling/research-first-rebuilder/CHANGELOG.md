# Changelog

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/), Versionierung nach SemVer.

## [1.1.0] - 2026-07-28

### Geaendert

- Evidenzdisziplin praezisiert: Quellen sind so zu notieren, dass ein Dritter sie ohne
  Rueckfrage wiederfindet - vollstaendige `https://`-URL plus, bei Repositories, Commit- und
  Blob-SHA. Ausloeser war die erste Eval-Runde: der Skill lieferte auf einen Commit gepinnte,
  sehr gut nachvollziehbare Quellenverzeichnisse, notierte sie aber als `github.com/org/repo`
  ohne Schema. Inhaltlich stark, fuer den Leser unnoetig muehsam.
- Template `02-quellenverzeichnis.md` verlangt die Spalte jetzt explizit in dieser Form.

### Eval-Ergebnis Iteration 1 (4 Testfaelle, je mit und ohne Skill, real ausgefuehrt)

| Konfiguration | bestandene Assertions |
|---|---|
| mit Skill | 28/28 |
| ohne Skill (Baseline) | 27/28 |

Der Abstand ist duenn und ehrlich als solcher zu lesen: die Assertion-Menge ist nahe der
Saettigung und damit wenig trennscharf. Zwei Befunde sind trotzdem belastbar - der
Nicht-Trigger-Fall (simpler Bugfix) loest den 10-Gate-Workflow korrekt **nicht** aus, und der
Ablehnungsfall zeigt **keinen** Vorteil gegenueber der Baseline. Zwei zunaechst gemessene
Vorteile stellten sich als Fehler im Grader heraus und wurden korrigiert, statt sie stehen zu
lassen.

## [1.0.0] — 2026-07-28

### Hinzugefügt

- Erste Fassung von `research-first-rebuilder`.
- Verbindlicher 10-Gate-Workflow; vor Abschluss von Gate 5 (Fork/Reuse/Build-Entscheidung)
  beginnt keine größere Produktimplementierung.
- Evidenzklassen OBSERVED / SOURCE-CLAIM / INFERRED / UNKNOWN mit ausdrücklicher Erlaubnis,
  `UNKNOWN` als vollwertiges Ergebnis zu melden statt Lücken zu füllen.
- Clean-Room-Regeln inklusive Grenzfällen (`references/clean-room.md`).
- Lizenzleitfaden mit den Fallen, die in der Praxis übersehen werden: Stub-LICENSE,
  „Apache mit Zusatzbedingungen", Open-Core-Unterbäume, von der Lizenz ausgenommene Marken,
  Modell- und Datensatzlizenzen (`references/licensing.md`).
- Bewertungsgewichtung mit harten Ausschlusskriterien, die eine hohe Punktzahl außer Kraft
  setzen (`references/evaluation.md`).
- Recherchemethodik: Quellenhierarchie, Umgang mit Widersprüchen, negative Befunde
  (`references/research-method.md`).
- 12 Templates für Auftrag, Produktanalyse, Quellenverzeichnis, Feature-Parität,
  OSS-Reuse-Matrix, Lizenzprüfung, Architekturentscheidung, Implementierungsplan,
  Threat Model, Traceability, Akzeptanzkriterien und Abschlussbericht.
- Hilfsskripte `license_scan.py`, `repo_health.py`, `verify_artifacts.py`.
- PowerShell-Installation und -Deinstallation.

### Bekannte Grenzen

- `license_scan.py` erkennt Lizenzen über Textsignaturen. Es ersetzt keine juristische
  Prüfung und trifft ausdrücklich keine Entscheidung.
- `repo_health.py` deckt GitHub ab; GitLab und andere Forges sind manuell zu prüfen.
- `verify_artifacts.py` prüft Existenz und Mindestgröße, nicht die inhaltliche Qualität.
