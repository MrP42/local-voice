# Lizenzprüfung

> Rechtlicher Hinweis: Dies ist eine Arbeitshilfe für technische Entscheidungen, keine
> Rechtsberatung. Bei kommerzieller Weitergabe im Zweifel juristisch prüfen lassen.

## Grundregel

**Lies die LICENSE-Datei direkt aus dem Repository am untersuchten Commit.**

Nicht verlassen auf: Suchergebnis-Snippets · das GitHub-Sidebar-Label (heuristisch, oft falsch) ·
Badges in der README · Angaben in Paketregistries · das eigene Gedächtnis.

Notiere immer: Repo-URL, Commit-SHA, Blob-SHA oder Dateigröße, exakter Lizenzname, genannter
Rechteinhaber und Jahr.

## Fallen, die regelmäßig übersehen werden

| Falle | Erkennungsmerkmal | Konsequenz |
|---|---|---|
| **Stub-LICENSE** | Datei nennt nur „AGPLv3" + Link, Volltext fehlt, kein Rechteinhaber | Rechtlich unklar → wie „keine Lizenz" behandeln |
| **Apache mit Zusatzbedingungen** | „Apache-2.0" plus zusätzliche Absätze | Nicht OSI-Apache. Oft Wettbewerbs- oder Deployment-Verbote |
| **Open Core** | `enterprise/`, `ee/`, `pro/` mit eigener LICENSE | Teilbaum ist proprietär, trotz Repo-Label |
| **Marke ausgenommen** | „Name, logo and icon are not covered by this license" | Branding **muss** ersetzt werden, auch bei MIT |
| **Dual License** | „MIT OR Apache-2.0" | Du darfst wählen — dokumentiere die Wahl |
| **Lizenzwechsel in der Historie** | ältere Commits andere Lizenz | Der Commit, den du forkst, zählt |
| **Abhängigkeiten ≠ Projekt** | Projekt MIT, Dependency GPL | Der Gesamtbaum entscheidet |
| **Modellgewichte ≠ Code** | Code Apache, Gewichte CC-BY-NC | Getrennt prüfen |
| **Vendored Code** | `third_party/`, `vendor/` | Eigene Lizenzen, eigene Pflichten |

## Lizenzfamilien und Pflichten

| Familie | Beispiele | Private Nutzung | Weitergabe |
|---|---|---|---|
| **Permissiv** | MIT, BSD-2/3, ISC, Apache-2.0, Unlicense, BSL-1.0 | keine Pflichten | Copyright-Hinweis + Lizenztext mitliefern. Apache-2.0 zusätzlich: `NOTICE` erhalten, Änderungen kennzeichnen, Patentklausel |
| **Schwaches Copyleft** | LGPL, MPL-2.0, EPL | keine Pflichten | Änderungen an den lizenzierten **Dateien** offenlegen; eigener Code darf proprietär bleiben. LGPL: Austauschbarkeit der Bibliothek sicherstellen |
| **Starkes Copyleft** | GPL-2.0, GPL-3.0 | keine Pflichten | Bei **jeder** Weitergabe: vollständiger Quellcode des Gesamtwerks unter GPL |
| **Netzwerk-Copyleft** | AGPL-3.0 | keine Pflichten | Wie GPL, **zusätzlich** löst Bereitstellung über ein Netzwerk die Offenlegungspflicht aus (§13) |
| **Quelloffen, nicht Open Source** | BUSL, SSPL, Elastic, Commons Clause | meist erlaubt | Meist Verbot konkurrierender/gehosteter Angebote. Sorgfältig lesen |
| **Keine Lizenz** | — | **nichts erlaubt** | Alle Rechte vorbehalten. Nicht kopieren, nicht forken |

Für ein rein lokales Werkzeug, das nie weitergegeben wird, erzeugt selbst AGPL praktisch keine
Pflichten. Es verbaut aber jede spätere Veröffentlichung — deshalb bei `distribution-mode: private`
trotzdem beide Fälle bewerten.

## Content- und Modelllizenzen

| Lizenz | Kernpflicht |
|---|---|
| CC0 | keine |
| **CC-BY-4.0** | **Namensnennung des Urhebers** — gehört in „Über"/Third-Party-Notices |
| CC-BY-SA | Namensnennung + Weitergabe unter gleichen Bedingungen |
| CC-BY-NC | **kein kommerzieller Einsatz** — bei kommerzieller Absicht Ausschluss |
| OpenRAIL / Llama / Gemma-artig | Nutzungsbeschränkungen, teils Nutzerzahl-Schwellen |
| OpenMDW | meist kommerziell nutzbar, Bedingungen lesen |

Modellkarten sind nicht verbindlich — die Lizenzdatei im Modell-Repo zählt.

## Kompatibilität beim Kombinieren

Sichere Richtung: **permissiv → copyleft**. Umgekehrt nicht.

- MIT/BSD/ISC lassen sich fast überall einbinden
- Apache-2.0 ist mit GPL-2.0 **nicht** kompatibel (Patentklausel), mit GPL-3.0 schon
- GPL und AGPL nicht in ein permissiv lizenziertes Produkt ziehen, wenn dieses permissiv bleiben soll
- Bei jeder Kombination: Lizenzkompatibilität **vor** der Übernahme prüfen, nicht danach

## Pflichtartefakte vor Weitergabe

1. **THIRD-PARTY-NOTICES** — je Komponente: Name, Version, Lizenz, Copyright-Hinweis, Volltext
   oder Link. Auch Modelle und Datensätze aufführen.
2. **SBOM** — z. B. `cargo about`, `cargo deny`, `syft`, `license-checker`
3. **Advisory-Scan** — `cargo audit`, `npm audit`, GitHub Advisory DB
4. **Upstream-Dokumentation** bei Fork: Repo, Commit, Lizenz, Liste der Abweichungen
5. **Branding-Prüfung** — sind Name/Logo/Icons des Upstream ausgenommen?
