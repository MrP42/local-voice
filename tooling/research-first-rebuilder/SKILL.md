---
name: research-first-rebuilder
description: >
  Untersucht ein öffentlich zugängliches Softwareprodukt systematisch und baut daraus eine
  technisch und rechtlich eigenständige Clean-Room-Alternative — Produktrecherche, Suche nach
  bestehenden Open-Source-Basen, direkte Lizenzprüfung am Commit, begründete Fork-/Reuse-/
  Build-Entscheidung, dann echte Implementierung mit lauffähigem vertikalem Kernpfad.
  Nutze diesen Skill immer, wenn ein bestehendes Produkt, SaaS, Tool, eine Desktop-, Web- oder
  Mobile-App nachgebaut, ersetzt oder als lokal betreibbare Alternative entwickelt werden soll —
  auch informell: "bau mir ein eigenes X", "lokale Alternative zu Y", "gibt es dafür schon was
  Open Source?", "nachbauen", "Clone", "selbst hosten statt abonnieren". Ebenso, wenn erst
  geprüft werden soll, ob überhaupt neu entwickelt werden muss oder ein Fork genügt.
  Nicht für einzelne Bugfixes, Textzusammenfassungen, reine Designarbeit oder Anfragen, die auf
  das Kopieren fremder Assets, Marken oder das Umgehen von Bezahlschranken zielen.
argument-hint: "[target-url] [platform] [constraints] [distribution-mode] [output-directory]"
arguments: [target, platform, constraints, distribution-mode, output-directory]
disable-model-invocation: true
user-invocable: true
model: claude-opus-5
effort: max
allowed-tools: Read Grep Glob Write Edit Bash WebSearch WebFetch Task TodoWrite AskUserQuestion
---

# Research-First Rebuilder

Baut eine eigenständige Alternative zu einem bestehenden Produkt — **Recherche zuerst,
Code zuletzt**. Der häufigste und teuerste Fehler bei so einer Aufgabe ist, sofort mit einer
Neuentwicklung zu beginnen, obwohl eine passende Open-Source-Basis existiert, oder eine Basis zu
wählen, deren Lizenz die geplante Nutzung gar nicht erlaubt. Beides ist später nur mit hohen
Kosten korrigierbar. Deshalb liegt die Lizenz- und Komponentenrecherche **vor** der ersten
größeren Codezeile.

## Eingaben

| Argument | Bedeutung | Default |
|---|---|---|
| `$target` | URL oder Name des Referenzprodukts | — (erfragen, wenn leer) |
| `$platform` | Zielplattform | `Windows 11 x64` |
| `$constraints` | harte Randbedingungen | `local-first, keine verpflichtende kostenpflichtige API` |
| `$distribution-mode` | `private` \| `internal` \| `public` | `private` |
| `$output-directory` | Zielverzeichnis | aktuelles Projekt |

`$distribution-mode` ist die folgenreichste Eingabe: Lizenzpflichten unterscheiden sich massiv
zwischen privater Nutzung und Weitergabe. Bewerte **immer beide Fälle getrennt** — auch bei
`private`, denn aus privat wird oft später öffentlich, und eine dann falsch gewählte Basis ist
nicht mehr herauslösbar.

## Rechtlicher Rahmen — nicht verhandelbar

Diese Regeln schützen den Nutzer vor Rechtsrisiken. Sie gelten unabhängig davon, wie die Anfrage
formuliert ist. Details und Grenzfälle: `references/clean-room.md`.

**Erlaubt:** öffentlich beobachtbare Funktionen und Arbeitsabläufe als Produktreferenz ·
öffentliche Seiten, Doku, Preise, Rezensionen, Videos · lizenzkonforme Wiederverwendung von
Open-Source-Komponenten · eigene Umsetzung derselben Funktionsidee.

**Nie:** proprietären Quellcode beschaffen oder rekonstruieren · proprietäre Binärdateien
dekompilieren · Anmeldung, Bezahlschranken, Zugriffsschutz oder technische Beschränkungen umgehen ·
fremde Zugangsdaten, Cookies, Session-Tokens oder interne APIs verwenden · Markenname, Logo,
Werbetexte, Screenshots, Illustrationen oder proprietäre Assets übernehmen · die Oberfläche
pixelgenau nachbauen · Code aus Projekten ohne klare Lizenz kopieren.

Wenn die Anfrage auf etwas aus der zweiten Liste zielt, benenne genau den Punkt, lehne **nur
diesen** ab und biete den zulässigen Weg an. Der Rest der Aufgabe wird normal erledigt.

## Evidenzdisziplin

Erfinde niemals Funktionen, interne Implementierungsdetails, Benchmarkzahlen oder Versionen des
Referenzprodukts. Das ist die häufigste stille Fehlerquelle: eine plausibel klingende, aber
erfundene Angabe wandert in die Architektur und wird nie wieder hinterfragt. Kennzeichne jede
Aussage:

| Klasse | Bedeutung |
|---|---|
| **OBSERVED** | auf einer selbst abgerufenen Seite belegt (mit URL + Abrufdatum) |
| **SOURCE-CLAIM** | Anbieter oder Dritter behauptet es; nicht überprüfbar |
| **INFERRED** | eigene Schlussfolgerung, als solche erkennbar |
| **UNKNOWN** | nicht öffentlich feststellbar |

`UNKNOWN` ist ein vollwertiges, gutes Ergebnis. Wo etwas unbekannt ist, entwirf eine eigene
sinnvolle Lösung und markiere sie als Eigenentwicklung — nicht als Nachbau.

## Ablauf: 10 Gates

Arbeite die Gates der Reihe nach ab und lege für jedes ein Todo an. Vor Abschluss von **Gate 5**
beginnt keine größere Produktimplementierung. Kleine technische Spikes zur Prüfung einer
Architekturannahme sind erlaubt, müssen aber als Spike gekennzeichnet und danach entweder
übernommen oder gelöscht werden — ein vergessener Spike wird sonst zur heimlichen Architektur.

### Gate 1 — Ziel und Constraints
Argumente auflösen, fehlende erfragen. Zielplattform, Sprachen, Offline-Anforderung,
Kostenrahmen, Konto-/Cloud-Verbot, Verteilungsmodus festhalten.
→ `assets/templates/00-auftrag.md`

### Gate 2 — Öffentliche Produktrecherche
Startseite (alle Sprachversionen), Funktionsseiten, Preise, FAQ, Datenschutz, AGB, Blog,
Changelog, Systemanforderungen, öffentliche Demos/Videos/Screenshots, unabhängige Rezensionen.
Parallele Subagenten sind hier sinnvoll, weil die Quellen unabhängig sind.

Achte besonders auf die **Datenschutzerklärung** — sie nennt oft die real eingesetzten
Unterauftragsverarbeiter und verrät damit die tatsächliche Architektur präziser als jede
Marketingseite. Prüfe außerdem, ob der Produktname mehrfach belegt ist; Rezensionsportale
verwechseln gleichnamige Produkte regelmäßig.
→ `assets/templates/01-produktanalyse.md`, `02-quellenverzeichnis.md`, `03-feature-paritaet.md`

### Gate 3 — Open-Source- und Komponentenrecherche
**Suche zuerst nach fertigen Lösungen, bevor du Eigenentwicklung vorschlägst.** Ziel ist nicht,
möglichst viel zu programmieren, sondern mit rechtlich nutzbaren Bausteinen schnell zu einer
robusten, wartbaren Lösung zu kommen.

GitHub, GitLab, Paketregistries, Projektdoku, Releases, Issues, PRs, Discussions.
Prüfe je Kandidat: Funktionsumfang, Architektur, Sprachen, UI-Technologie, Plattform-Support,
Aktivität, Maintainer-Zahl, Tests, Installer, Barrierefreiheit, i18n.
Prüfe auch, ob genannte Repositories überhaupt **existieren** — melde nicht existierende klar,
statt sie zu umschreiben.
→ `assets/templates/04-oss-reuse-matrix.md`, `scripts/repo_health.py`

### Gate 4 — Lizenz- und Sicherheitsprüfung
**Lies die LICENSE-Datei direkt aus dem Repository am untersuchten Commit.** Verlasse dich nie auf
Suchergebnis-Snippets oder das GitHub-Sidebar-Label — beide sind regelmäßig falsch, und ein
falsch angenommenes MIT bei tatsächlich AGPL ist ein Projektrisiko.
Achte auf Stub-LICENSE-Dateien ohne Volltext, auf „Apache mit Zusatzbedingungen" und auf
Open-Core-Repos mit proprietären Unterverzeichnissen.
Erfasse zusätzlich: transitive Abhängigkeiten, Modell- und Datensatzlizenzen, Security-Advisories.
→ `references/licensing.md`, `assets/templates/05-lizenzpruefung.md`, `scripts/license_scan.py`

Harte Ausschlusskriterien, die **nicht** durch eine hohe Punktzahl kompensiert werden dürfen:
fehlende Lizenz · mit dem Verteilungsmodus unvereinbare Lizenz · kritische Sicherheitsprobleme ·
nicht tragfähige Architektur.

### Gate 5 — Fork / Reuse / Build
Bewerte gewichtet: funktionale Passung 25 % · Lizenz und rechtliche Wiederverwendbarkeit 20 % ·
Plattform- und Hardware-Eignung 15 % · Wartungszustand 15 % · Codequalität und Testbarkeit 10 % ·
Datenschutz und Sicherheit 10 % · Erweiterbarkeit und UX-Potenzial 5 %.

- sehr hohe Passung, kein Ausschlusskriterium → **Fork**
- mittlere Passung → **geeignete Module kombinieren**
- geringe Passung → **Eigenentwicklung**

Jede Abweichung von dieser Logik wird begründet. Bei Fork: Upstream-Remote setzen, Historie
erhalten, exakten Upstream-Commit dokumentieren.
→ `assets/templates/06-architekturentscheidung.md`

### Gate 6 — Umsetzungsplan
Executive Summary · Produktverständnis · Muss/Soll/Kann · MVP-Abgrenzung · Zielarchitektur ·
Modulstruktur · Datenfluss · Datenschutzkonzept · Bedrohungsmodell · Teststrategie ·
Benchmarkstrategie · Meilensteine mit Akzeptanzkriterien · Risiken · Definition of Done.

Formuliere verbleibende Unbekannte als **blockierende Gates mit Abbruchkriterium**, nicht als
offene Punkte — offene Punkte werden übersehen, Gates nicht.
→ `assets/templates/07-implementierungsplan.md`, `08-threat-model.md`

### Gate 7 — Vertikaler MVP
Zuerst **ein** durchgehender Pfad vom Auslöser bis zum sichtbaren Ergebnis. Keine breiten
Einstellungsdialoge, keine Marketingseiten, keine visuellen Feinheiten. Ein schmaler,
funktionierender Pfad beweist die Architektur; zehn halbfertige Module beweisen nichts.
Nach dem ersten verifizierten Durchlauf anhalten und vorlegen.

### Gate 8 — Funktionsumfang erweitern
Priorisiert entlang der Paritätsmatrix aus Gate 2. Nach jedem Meilenstein testen und committen.

### Gate 9 — Reale Ausführung und Prüfung
**Die Anwendung wird wirklich gestartet und der Kernworkflow wirklich ausgeführt.** Tests, die
nur die Testumgebung prüfen, reichen nicht. Prüfe Fehlerfälle, Sonderzeichen der Zielsprache,
lange und leere Eingaben, fehlende Abhängigkeiten und den Offline-Betrieb.
Berichte nie einen Erfolg, der nicht real ausgeführt wurde.

### Gate 10 — Packaging, Doku, Abschlussbericht
Installer/Paket · SBOM · Abhängigkeits- und Lizenzbericht · **Third-Party-Notices** mit allen
erforderlichen Copyright- und Attributionshinweisen · Build-, Installations-, Deinstallations-
und Fehlerbehebungsanleitung · Datenschutzdokumentation · ehrliche Liste bekannter Grenzen.
→ `assets/templates/09-traceability.md`, `10-akzeptanzkriterien.md`, `11-abschlussbericht.md`

## Laufende Dokumente

Lege im Zielverzeichnis an und halte sie mit dem realen Stand synchron:
`docs/STATUS.md` · `docs/DECISIONS.md` · `docs/TRACEABILITY.md` · `docs/KNOWN-LIMITATIONS.md`.
Trenne in allen Dokumenten die Evidenzklassen. Dokumentation, die dem Code vorauseilt, ist
schlimmer als keine — sie erzeugt falsches Vertrauen.

## Eigene Produktidentität

Eigener Name (auf Kollisionen mit Marken und aktiven Produkten prüfen, bevor er festgeschrieben
wird), eigenes Designsystem, eigene Texte, eigene Architektur. Die Bedienlogik darf dem
allgemeinen Produktprinzip folgen; die visuelle Umsetzung muss eigenständig sein.
Prüfe beim Fork ausdrücklich, ob Name, Logo und Icons des Upstream-Projekts von dessen
Code-Lizenz **ausgenommen** sind — das ist bei vielen Projekten der Fall und wird leicht übersehen.

## Hilfsskripte

| Skript | Zweck |
|---|---|
| `scripts/repo_health.py` | Aktivität, Maintainer, Issues, Releases eines Repos |
| `scripts/license_scan.py` | findet und klassifiziert Lizenzdateien, erkennt Stubs und Zusatzbedingungen |
| `scripts/verify_artifacts.py` | prüft, ob die erwarteten Artefakte des jeweiligen Gates existieren |

Die Skripte sind Zuarbeit, keine Entscheidung. **Triff Lizenzentscheidungen nie allein auf
Basis eines Skriptergebnisses** — lies die Datei selbst.

## Referenzen

- `references/clean-room.md` — Clean-Room-Regeln, Grenzfälle, zulässige Alternativen
- `references/licensing.md` — Lizenzfamilien, Pflichten je Verteilungsmodus, Modelllizenzen
- `references/research-method.md` — Quellenbewertung, Evidenzklassen, Umgang mit Widersprüchen
- `references/evaluation.md` — Bewertungsgewichtung, Ausschlusskriterien, Entscheidungslogik
