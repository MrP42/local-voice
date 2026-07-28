# Architekturentscheidung: Fork / Reuse / Build (Gate 5)

## Bewertung

| Kriterium | Gewicht | Kandidat A | Kandidat B | Kandidat C |
|---|---|---|---|---|
| Funktionale Passung | 25 % | | | |
| Lizenz / rechtliche Wiederverwendbarkeit | 20 % | | | |
| Plattform- und Hardware-Eignung | 15 % | | | |
| Wartungszustand | 15 % | | | |
| Codequalität / Testbarkeit | 10 % | | | |
| Datenschutz / Sicherheit | 10 % | | | |
| Erweiterbarkeit / UX-Potenzial | 5 % | | | |
| **Gewichtete Summe** | | | | |

Punkte 0–5 je Kriterium. **Harte Ausschlüsse setzen die Punktzahl außer Kraft** — ein Kandidat
mit 4,6/5 und fehlender Lizenz ist ausgeschlossen, nicht „knapp dahinter".

## Entscheidung

**Gewählt:** Fork / Module kombinieren / Eigenentwicklung
**Basis:** `<repo>` @ `<commit>`
**Begründung:**

**Abweichung von der Standard-Entscheidungslogik?** (falls ja, begründen)

## Nachteile, die wir bewusst mitkaufen

_Ehrlich benennen — sie tauchen sonst später als Überraschung auf._

## Bei Fork: Pflichten

- [ ] Upstream-Remote gesetzt
- [ ] Git-Historie erhalten
- [ ] Commit-SHA in `UPSTREAM.md` und `docs/DECISIONS.md`
- [ ] Lizenz- und Copyright-Hinweise erhalten
- [ ] Branding ersetzt (**Name/Logo/Icons sind oft von der Code-Lizenz ausgenommen**)
- [ ] Telemetrie / Cloud / Updater geprüft und entfernt oder umgeleitet
- [ ] Problematische Abhängigkeiten aktualisiert
- [ ] Abweichungen vom Upstream dokumentiert
- [ ] Keine unnötige Komplettneuschreibung
