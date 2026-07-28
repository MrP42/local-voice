# Changelog

Format nach [Keep a Changelog](https://keepachangelog.com/de/1.1.0/), Versionierung nach SemVer.

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
