# research-first-rebuilder

Ein Claude-Code-Skill, der ein öffentlich zugängliches Softwareprodukt systematisch untersucht
und daraus eine technisch und rechtlich eigenständige Alternative entwickelt — **Recherche
zuerst, Code zuletzt**.

## Wozu

Bei „bau mir ein eigenes X" sind zwei Fehler teuer und häufig:

1. **Sofort selbst bauen**, obwohl eine passende Open-Source-Basis existiert.
2. **Eine Basis wählen, deren Lizenz die geplante Nutzung nicht erlaubt** — und das erst
   bemerken, wenn das Produkt fertig ist.

Der Skill stellt beides vor die erste größere Codezeile: Produktrecherche, OSS-Suche, direkte
Lizenzprüfung am Commit, begründete Fork-/Reuse-/Build-Entscheidung — und erst danach eine
echte Implementierung mit lauffähigem vertikalem Kernpfad.

## Installation

```powershell
.\install.ps1
```

Installiert nach `~\.claude\skills\research-first-rebuilder\`. Claude Code erkennt neue Skills
unter diesem Pfad innerhalb der laufenden Sitzung; erscheint der Befehl nicht, hilft eine neue
Sitzung.

Entfernen:

```powershell
.\uninstall.ps1
```

Die Quelle unter `tooling/` bleibt dabei unberührt — sie ist die einzige Wahrheitsquelle, die
Installation ist nur ein Deployment-Artefakt.

## Aufruf

```
/research-first-rebuilder <target-url> [platform] [constraints] [distribution-mode] [output-directory]
```

Beispiel:

```
/research-first-rebuilder https://beispiel.de "Windows 11 x64" "local-first, offline, kein Konto" private apps/mein-tool
```

| Argument | Default |
|---|---|
| `target` | — (wird erfragt) |
| `platform` | `Windows 11 x64` |
| `constraints` | `local-first, keine verpflichtende kostenpflichtige API` |
| `distribution-mode` | `private` |
| `output-directory` | aktuelles Projekt |

`distribution-mode` ist die folgenreichste Eingabe — Lizenzpflichten unterscheiden sich stark
zwischen privater Nutzung und Weitergabe. Der Skill bewertet immer beide Fälle.

## Ablauf

Zehn Gates. Vor Abschluss von **Gate 5** (Fork/Reuse/Build) beginnt keine größere
Produktimplementierung.

| Gate | Inhalt |
|---|---|
| 1 | Ziel und Constraints |
| 2 | Öffentliche Produktrecherche |
| 3 | Open-Source- und Komponentenrecherche |
| 4 | Lizenz- und Sicherheitsprüfung |
| 5 | **Fork / Reuse / Build** |
| 6 | Umsetzungsplan |
| 7 | Vertikaler MVP |
| 8 | Funktionsumfang erweitern |
| 9 | Reale Ausführung und Prüfung |
| 10 | Packaging, Doku, Abschlussbericht |

## Aufbau

```
SKILL.md                  Workflow und Regeln
references/
  clean-room.md           Was erlaubt ist, was nie, und die Grenzfälle
  licensing.md            Lizenzfamilien, Pflichten, die üblichen Fallen
  research-method.md      Quellenhierarchie, Evidenzklassen, Widersprüche
  evaluation.md           Gewichtung, Ausschlusskriterien, Entscheidungslogik
assets/templates/         12 Vorlagen, je Gate
scripts/
  license_scan.py         findet und klassifiziert Lizenzdateien, erkennt Stubs/Zusatzklauseln
  repo_health.py          Aktivität, Maintainer, Releases (GitHub)
  verify_artifacts.py     prüft, ob die Artefakte eines Gates real existieren
evals/                    Testfälle (nicht Teil der Installation)
```

## Clean-Room

Öffentlich erkennbare Funktionen und Arbeitsabläufe dürfen als Produktreferenz dienen.
Nicht übernommen werden: proprietärer Quellcode, dekompilierte Binärdateien, Marken, Logos,
Werbetexte, Screenshots, Assets, pixelgenaue Oberflächen. Keine Umgehung von Anmeldung,
Bezahlschranken oder Zugriffsschutz; keine fremden Zugangsdaten.

Details: `references/clean-room.md`.

## Grenzen

- `license_scan.py` arbeitet mit Textsignaturen und ist eine Zuarbeit — **keine** juristische
  Prüfung und keine Entscheidung. Die LICENSE-Datei wird immer selbst gelesen.
- `repo_health.py` deckt GitHub ab; andere Forges sind manuell zu prüfen.
- `verify_artifacts.py` prüft Existenz und Mindestgröße, nicht inhaltliche Qualität.

Version: siehe `VERSION` · Änderungen: siehe `CHANGELOG.md`
