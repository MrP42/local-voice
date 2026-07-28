# Datenschutz- und Bedrohungsmodell (Gate 6)

## Schutzwürdige Daten

| Datenart | Wo gespeichert | Wie lange | Löschbar |
|---|---|---|---|

## Datenflussdiagramm

```
```

## Vollständige Liste möglicher Netzwerkzugriffe

| Ziel | Wann | Ausgelöst durch | Abschaltbar |
|---|---|---|---|

_Diese Liste ist die Grundlage des Offline-Tests: Was hier nicht steht, darf im Test nicht
auftreten. Ein Offline-Test ohne diese Liste prüft nichts._

## Bedrohungen

| # | Bedrohung | Auswirkung | Gegenmaßnahme | Verifiziert durch |
|---|---|---|---|---|

Typische Kandidaten je nach Produkt: unbeabsichtigter Datenabfluss an Dritte · SSRF durch
nutzerkonfigurierte Endpunkte · Manipulation heruntergeladener Artefakte · Secrets in Logs
oder Diagnoseexporten · Persistenz trotz deaktivierter Historie · Prompt Injection über
verarbeitete Inhalte.

## Datenschutz-Defaults

- [ ] Keine Telemetrie
- [ ] Kein Benutzerkonto
- [ ] Keine Cloud-Aufrufe im Offline-Modus
- [ ] Sensible Werte über den Plattform-Mechanismus geschützt (nicht im Klartext)
- [ ] Keine Secrets im Repository
- [ ] Logs ohne Inhaltsdaten, außer explizit für Diagnose aktiviert

## Löschkonzept

| Was | Wo (konkreter Pfad) | Wie gelöscht |
|---|---|---|

_Auch bei Deinstallation: alle Orte einzeln aufzählen. Nutzergewählte externe Pfade nur nach
ausdrücklicher Bestätigung löschen._
