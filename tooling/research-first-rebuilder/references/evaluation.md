# Bewertung und Entscheidung

## Gewichtung

| Kriterium | Gewicht | Was zählt |
|---|---|---|
| Funktionale Passung | 25 % | Deckt es den Kernworkflow? Wie viel fehlt? |
| Lizenz / rechtliche Wiederverwendbarkeit | 20 % | Passt sie zum Verteilungsmodus — heute und später? |
| Plattform- und Hardware-Eignung | 15 % | Zielplattform, Beschleunigung, Fallback |
| Wartungszustand | 15 % | Commits, aktive Maintainer, Reaktion auf Issues, Releases |
| Codequalität und Testbarkeit | 10 % | Tests, CI, Struktur, Lesbarkeit |
| Datenschutz und Sicherheit | 10 % | Telemetrie, Kontozwang, Cloud-Kopplung, Advisories |
| Erweiterbarkeit und UX-Potenzial | 5 % | Modulgrenzen, Plugin-Punkte, i18n, Barrierefreiheit |

Punkte je Kriterium 0–5, gewichtet summieren. Die Zahl ordnet Kandidaten — sie entscheidet nicht.

## Harte Ausschlusskriterien

Diese setzen die Punktzahl außer Kraft. Ein Kandidat mit 4,6/5 und fehlender Lizenz ist
**ausgeschlossen**, nicht „knapp dahinter":

- keine Lizenz oder rechtlich unklare Lizenz (inkl. Stub-Dateien)
- Lizenz unvereinbar mit dem Verteilungsmodus
- kritische, nicht behebbare Sicherheitsprobleme
- Architektur trägt die Anforderung grundsätzlich nicht
- verpflichtendes Konto, Aktivierung oder Phone-Home, wenn „kein Konto" gefordert ist

Der Grund für die Härte: Punktzahlen laden dazu ein, ein rechtliches Problem gegen technische
Eleganz aufzurechnen. Das geht am Ende immer schlecht aus.

## Entscheidungslogik

| Lage | Entscheidung |
|---|---|
| Sehr hohe Passung, kein Ausschlusskriterium | **Fork** |
| Mittlere Passung, gute Module | **Module kombinieren** |
| Geringe Passung oder alle Kandidaten ausgeschlossen | **Eigenentwicklung** |

Jede Abweichung begründen.

**Das Ziel ist nicht, möglichst viel selbst zu programmieren.** Das Ziel ist, mit rechtlich
nutzbaren Bausteinen schnell zu einer robusten, wartbaren, eigenständigen Lösung zu kommen.
Eine Eigenentwicklung „weil sauberer" ist fast immer teurer als gedacht — begründe sie mit
Fakten aus der Recherche, nicht mit Geschmack.

Umgekehrt gilt: ein Fork mit 200 offenen Issues, rotem Testlauf und drei gepinnten
Maintainer-Forks als Abhängigkeiten ist auch dann teuer, wenn die Lizenz stimmt. Nenne die
Nachteile, die du mitkaufst, ausdrücklich — sie tauchen sonst später als Überraschung auf.

## Fork-Pflichten

Bei Entscheidung „Fork":

1. Upstream-Remote setzen, Git-Historie erhalten
2. Upstream-Commit-SHA dokumentieren (`UPSTREAM.md` + `docs/DECISIONS.md`)
3. Lizenz und Copyright-Hinweise erhalten
4. Branding ersetzen — Name, Logo, Icons sind häufig von der Code-Lizenz **ausgenommen**
5. Cloud-, Telemetrie- und Updater-Komponenten prüfen und entfernen oder umleiten
6. Problematische Abhängigkeiten aktualisieren
7. Abweichungen vom Upstream dokumentieren
8. Keine unnötige Komplettneuschreibung — dann wäre der Fork sinnlos gewesen

## Modulkombination

Bei Entscheidung „Module kombinieren":

1. Lizenzkompatibilität **vor** der Übernahme prüfen
2. Nur klar abgegrenzte Komponenten übernehmen
3. Herkunft und Änderungen je Modul dokumentieren
4. Third-Party-Notices führen
5. Nichts aus Projekten ohne klare Lizenz übernehmen

Oft ist die beste Wiederverwendung nicht der Code, sondern die **Erkenntnis**: welche API ein
Projekt nutzt, welche Fallstricke es gelöst hat, welche Reihenfolge funktioniert. Das ist frei
verwendbar und häufig wertvoller als die Zeilen selbst.
