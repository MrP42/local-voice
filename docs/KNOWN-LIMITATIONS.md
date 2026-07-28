# Bekannte Einschränkungen

Ehrliche Liste. Was hier steht, ist nicht implementiert, nicht verifiziert oder prinzipiell
begrenzt — unabhängig davon, wie gut das Gegenteil klingen würde.

## Prinzipielle Grenzen der Plattform

| Grenze | Ursache | Umgang |
|---|---|---|
| **Einfügen in Fenster mit Administratorrechten schlägt fehl** | Windows UIPI verwirft Eingaben aus Prozessen niedrigerer Integritätsstufe **ohne Fehlermeldung** | Erkennen und offenlegen: Text bleibt in der Zwischenablage, Hinweis wird angezeigt. Behebbar nur über ein UIAccess-Manifest (erzwingt Code-Signatur **und** Installation in Program Files) oder Start als Administrator. Beides ist für v1 bewusst nicht gewählt. |
| **Der Erfolg einer Einfügung ist nicht messbar** | `SendInput` bestätigt nur das Einreihen von Eingaben, nicht die Übernahme durch die Zielanwendung | Kein automatisches Lernen aus Fehlschlägen. Stattdessen statische Strategietabelle, manuelle Übersteuerung pro Anwendung und ausdrückliches Nutzerfeedback. |
| **Die Zwischenablage lässt sich nicht vollständig wiederherstellen** | Delayed Rendering und OLE-Datenobjekte sind nicht rundreisefähig | Zugesagt werden ausschließlich `CF_UNICODETEXT`, `CF_HTML` und `CF_HDROP`. Exotische Formate gehen verloren. |
| **`RegisterHotKey` liefert kein Key-Up-Ereignis** | Von Windows so vorgesehen. `MOD_KEYUP` existiert nur in der archivierten Windows-CE-Dokumentation und gilt nicht für Desktop-Windows | Push-to-talk läuft über Low-Level-Keyboard-Hooks (`handy-keys`), abgesichert durch mehrere voneinander unabhängige Stopp-Auslöser. |
| **Ein Low-Level-Hook kann still entfernt werden** | Überschreitet er `LowLevelHooksTimeout`, entfernt Windows ihn **ohne jede Benachrichtigung** | Periodisches Neusetzen sowie Neusetzen nach Session-Unlock. |
| **Mikrofon kann still stumm bleiben** | Windows 11 hat einen separaten Schalter für Desktop-App-Mikrofonzugriff. Ist er aus, wird das Gerät weiterhin aufgelistet, liefert aber nur Stille | Erkennung über dauerhaft nahezu null Pegel, dann Verweis auf die Systemeinstellung. |

## Defekt: Live-Injektion des Streams (stream_injection)

**Standardmäßig abgeschaltet.** Die Funktion ist implementiert, kompiliert und
unit-getestet — liefert aber im realen Einsatz falschen Text.

Beobachtet 2026-07-29: Gesprochen wurde „Hi, mein Name ist Patrick Wolff.",
eingefügt wurde `Hi, mein ttttttt ffffffffffff`. Der Anfang stimmt, danach werden
einzelne Zeichen vielfach wiederholt. Zusätzlich kommt gar nichts an, wenn nicht
sofort nach dem Hotkey gesprochen wird.

Was bereits ausgeschlossen ist:
- Die Injektion feuert (Log zeigt Aufrufe, keine Fehler, kein Panic).
- Die Clipboard-Race war eine **erste, andere** Ursache und ist behoben — der
  Wechsel auf direktes Tippen hat das ursprüngliche Symptom („nur erster Satz")
  beseitigt und dieses neue erzeugt.

Noch nicht geklärt, in Reihenfolge der Wahrscheinlichkeit:
1. **Delta-Berechnung gegen `committed`.** Wird das bestätigte Präfix nicht rein
   angehängt, sondern zwischendurch neu formatiert oder gekürzt, ist der per
   Byte-Offset geschnittene Zuwachs falsch. Der Byte-Offset ist die verdächtigste
   Stelle im ganzen Mechanismus.
2. **`enigo.text()` bei schneller Folge.** Zeichenwiederholung ist ein typisches
   Artefakt fehlender bzw. verschluckter Key-Up-Ereignisse bei synthetischer
   Unicode-Eingabe.
3. **Startverhalten.** Dass ohne sofortiges Sprechen nichts ankommt, deutet auf
   einen Zustand, der zurückgesetzt wird, bevor der erste Zuwachs vorliegt.

Nächster Schritt wäre, den Rohwert von `committed` je Emission zu protokollieren
und mit dem tatsächlich getippten Fragment zu vergleichen — ohne diese Messung
ist jede weitere Änderung geraten.

## Noch nicht implementiert (Stand 2026-07-28)

- Der vollständige vertikale Pfad Hotkey bis Einfügen ist **noch nicht real durchlaufen**.
  Verifiziert ist bisher die **lokale Transkription**, nicht die Kette darum herum.
- Regelbasierte Nachbearbeitung, Texttreue-Validator, LLM-Stufe
- Injektions-Fallback-Kette und Strategietabelle pro Anwendung
- Wörterbuch, Snippets, Formatierungsprofile, Ausnahmen pro Anwendung
- Windows-Installer, SBOM, Third-Party-Notices
- Benchmarks über die eine gemessene Transkription hinaus
- Offline-Test mit Aufzeichnung der Netzwerkverbindungen

## Bewusst nicht umgesetzt

- **Kein Provider für die Claude-Code-CLI oder die Codex-CLI.** Anthropic untersagt es
  ausdrücklich, Anfragen über Abo-Anmeldedaten durch Dritt-Software zu leiten; OpenAIs Terms
  waren nicht abrufbar (HTTP 403 auf drei URLs). Siehe `DECISIONS.md` D3. Der generische
  OpenAI-kompatible Client erreicht beide Anbieter auf dem erlaubten Weg per nutzereigenem
  API-Schlüssel.
- **Kein Benutzerkonto, keine Telemetrie, kein Update-Server.**

## Unsicherheiten, die noch gemessen werden müssen

- Vulkan gegen CPU auf dieser Hardware. **Es existiert kein publizierter Benchmark** dazu, und
  die gemessenen 392 ms Inferenz auf reiner CPU stellen den praktischen Nutzen einer
  GPU-Beschleunigung für Diktatlängen ohnehin infrage.
- Deutsche Interpunktions- und Großschreibqualität über den einen Testsatz hinaus. Gängige
  WER-Benchmarks entfernen Interpunktion vor der Wertung und sagen darüber nichts aus.
- Verhalten der Injektion in Terminals, Electron-Anwendungen und über RDP.
