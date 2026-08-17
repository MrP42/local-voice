# Bekannte Einschränkungen

Ehrliche Liste. Was hier steht, ist nicht implementiert, nicht verifiziert oder prinzipiell
begrenzt — unabhängig davon, wie gut das Gegenteil klingen würde.

## Was „zuverlässig" hier bedeutet — und was nicht

Zugesagt wird genau dies: **eine erfolgreich beendete Aufnahme führt entweder zur vollständigen
Einfügung, oder der vollständige Text bleibt in der Zwischenablage und es erscheint eine
sichtbare Meldung.**

**Nicht** zugesagt wird eine garantierte automatische Texteingabe in jede Windows-Anwendung.
Das ist auf dieser Plattform nicht versprechbar (UIPI, erhöhte Prozesse, uneinheitliches
Verhalten gegenüber `SendInput`), und die Anwendung behauptet es auch nicht.

## Prinzipielle Grenzen der Plattform

| Grenze | Ursache | Umgang |
|---|---|---|
| **Der Hotkey funktioniert nicht, solange ein erhöhtes Fenster den Fokus hat** | Ein Low-Level-Keyboard-Hook in einem nicht erhöhten Prozess erhält keine Tastenereignisse, während ein Fenster höherer Integritätsstufe den Fokus besitzt (UIPI) | **Nicht behebbar ohne Erhöhung.** Es gibt keinen stillen Textverlust — es passiert schlicht nichts, und das Ausbleiben der Aufnahmeanzeige ist sichtbar. Wer dort diktieren will, muss Sprechstift als Administrator starten. Siehe Messung unten. |
| **Einfügen in Fenster mit Administratorrechten schlägt fehl** | Windows UIPI verwirft Eingaben aus Prozessen niedrigerer Integritätsstufe **ohne Fehlermeldung** | Wird **vorab erkannt**: `paste_guard` fragt das Token des Zielprozesses ab und versucht gar nicht erst einzufügen. Text bleibt in der Zwischenablage, Overlay-Meldung erscheint — nativ verifiziert, siehe unten. Behebbar nur über ein UIAccess-Manifest (erzwingt Code-Signatur **und** Installation in Program Files) oder Start als Administrator. Beides ist für v1 bewusst nicht gewählt. |
| **Der Erfolg einer Einfügung ist nicht messbar** | `SendInput` bestätigt nur das Einreihen von Eingaben, nicht die Übernahme durch die Zielanwendung | Alles Beobachtbare *um* den Versuch herum wird geprüft (Zielfenster, Fokus vorher und nachher, Rechtelage, Rücklesen der Zwischenablage). Die Übernahme selbst bleibt unbeobachtbar: **eine Anwendung, die Strg+V ignoriert, sieht für uns aus wie Erfolg.** Der Text steht dann noch im Verlaufsfenster. |
| **Die Zwischenablage lässt sich nicht vollständig wiederherstellen** | Delayed Rendering und OLE-Datenobjekte sind nicht rundreisefähig | Zugesagt werden ausschließlich `CF_UNICODETEXT`, `CF_HTML` und `CF_HDROP`. Exotische Formate gehen verloren. |
| **`RegisterHotKey` liefert kein Key-Up-Ereignis** | Von Windows so vorgesehen. `MOD_KEYUP` existiert nur in der archivierten Windows-CE-Dokumentation und gilt nicht für Desktop-Windows | Push-to-talk läuft über Low-Level-Keyboard-Hooks (`handy-keys`), abgesichert durch mehrere voneinander unabhängige Stopp-Auslöser. |
| **Ein Low-Level-Hook kann still entfernt werden** | Überschreitet er `LowLevelHooksTimeout`, entfernt Windows ihn **ohne jede Benachrichtigung** | Periodisches Neusetzen sowie Neusetzen nach Session-Unlock. |
| **Mikrofon kann still stumm bleiben** | Windows 11 hat einen separaten Schalter für Desktop-App-Mikrofonzugriff. Ist er aus, wird das Gerät weiterhin aufgelistet, liefert aber nur Stille | Erkennung über dauerhaft nahezu null Pegel, dann Verweis auf die Systemeinstellung. |

## Gemessen: Der Hotkey ist gegenüber erhöhten Fenstern blind

**OBSERVED 2026-08-17.** Log-Zuwachs der Anwendung nach genau einem simulierten
Hotkey (`keybd_event`, Strg-links + Windows-links):

| Vordergrundfenster | Log wuchs um |
|---|---|
| Task-Manager (erhöht, verifiziert über `TokenElevation`) | **0 Bytes** — zweimal reproduziert |
| kein erhöhtes Fenster | **8223 Bytes** |

Das ist keine Eigenheit der Simulation, sondern UIPI: Ein Low-Level-Hook in
einem Prozess niedrigerer Integritätsstufe bekommt die Ereignisse nicht.

**Folge für den Vertrag:** In dieser Lage entsteht kein stiller Textverlust,
weil gar keine Aufnahme beginnt oder endet. Der Zweig `TargetElevated` des
Guards ist über den Hotkey deshalb praktisch unerreichbar — erreichbar ist er
über `--toggle-transcription`, und genau so wird er auch getestet.

## Verifiziert: erhöhtes Ziel führt zu Zwischenablage plus Meldung

**OBSERVED 2026-08-17**, Aufnahme per CLI gestartet und gestoppt, Task-Manager
beim Stopp im Vordergrund:

```
paste guard: target=Some(PasteTarget { hwnd: 97263922, pid: 160960 })
             foreground=Some(97263922) self_elevated=false target_elevated=Some(true)
Automatic paste not performed: TargetElevated
Zwischenablage danach: "Der Termin ist am dritten Februar."
```

Kein Einfügeversuch, vollständiges Transkript in der Zwischenablage.

## Der abgesicherte Einfügepfad — was er prüft und was nicht

`paste_guard.rs` + `clipboard::paste_transcript_guarded()`, seit 2026-08-17.

**Geprüft, in dieser Reihenfolge:**

1. Zielfenster wurde beim Stop-Hotkey erfasst (sonst: Fallback `no_target`)
2. Das Vordergrundfenster ist immer noch dasselbe (sonst: `focus_changed`)
3. Der Zielprozess läuft nicht erhöht, während wir es nicht tun (sonst: `target_elevated`) —
   **eine nicht abfragbare Rechtelage gilt als erhöht**, also fail-closed
4. Der Text steht nachweislich in der Zwischenablage; geprüft durch Rücklesen, ein stiller
   Wiederholungsversuch (sonst: `clipboard_unverified`)
5. Genau **ein** Tastendruck — niemals eine Wiederholung, niemals eine zweite Strategie
6. Nach dem Einfügen erneut derselbe Fokus (sonst: `focus_changed_during_paste`)

Erst danach wird die vorherige Zwischenablage wiederhergestellt. Bei jedem Fallback bleibt der
Transkripttext bewusst liegen.

**Nicht geprüft:**

- ob die Zielanwendung den Tastendruck tatsächlich verarbeitet hat
- ob das eingefügte Ergebnis dem Transkript entspricht (kein Rücklesen aus fremden Fenstern)
- Fenster ohne Eingabefeld (z. B. der Explorer): Der Versuch läuft durch, meldet Erfolg, und
  der Text landet nirgends. Er bleibt aber im Verlauf erhalten.

## Defekt: Live-Injektion des Streams (stream_injection)

**Standardmäßig abgeschaltet, und seit 2026-08-17 doppelt gesperrt:** Sie läuft nur, wenn
`stream_injection` **und** `experimental_enabled` gesetzt sind. Ein alter Einstellungs-Store
mit `stream_injection: true` reicht nicht mehr aus — genau dieser Zustand lag am 2026-08-17
vor und hätte die defekte Funktion im Alltag aktiviert.

Die Funktion ist implementiert, kompiliert und unit-getestet — liefert aber im realen Einsatz
falschen Text.

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
ist jede weitere Änderung geraten. Die dafür nötigen STREAMDIAG-Zeilen mit Klartext
existieren nur noch in **Debug-Builds** (siehe unten), das ist für diese Messung
der richtige Ort.

## Datenschutz: Transkripte in Logdateien

Bis 2026-08-17 schrieb STREAMDIAG bei `debug_mode: true` vollständige Transkripte
im Klartext nach `handy.log` — im vorgefundenen Log standen 264 solcher Zeilen mit
Diktatinhalten des Nutzers.

Seither gilt: **Klartext nur in Debug-Builds.** Im Release-Build loggen STREAMDIAG,
der Segmenter, der Abschlusslog der Transkription und die Refinement-Ablehnung
ausschließlich Längen und Gate-Namen, unabhängig von `debug_mode`.

Nicht davon erfasst und weiterhin im Klartext gespeichert — bewusst, weil es die
Kernfunktion ist:

- `history.db` (Verlaufsdatenbank, Transkripte im Klartext)
- die WAV-Aufnahmen unter `%APPDATA%\de.wolffappliedai.sprechstift\recordings`

Beides ist die vom Nutzer gewollte Verlaufsfunktion und unterliegt der
Aufbewahrungseinstellung, nicht dem Logging.

**Die alte Logdatei wurde nicht automatisch gelöscht.** Wer die Altlast entfernen
will, löscht sie von Hand:
`%LOCALAPPDATA%\de.wolffappliedai.sprechstift\logs\handy.log`

## Noch nicht implementiert (Stand 2026-08-17)

- Abnahme gegen Browser-Textfeld, Microsoft Word und VS Code. Verifiziert ist bisher
  **Notepad** (UI-Automation-Rücklesung), der Explorer als Fenster ohne Eingabefeld und
  ein erhöhter Task-Manager.
- Der Segment-Modus (`segment_injection`, standardmäßig aus) nutzt weiterhin den **alten,
  ungeschützten** Einfügepfad `clipboard::paste`. Nur der Abschluss-Einfügevorgang der
  Standard-Diktatstrecke ist abgesichert.
- Regelbasierte Nachbearbeitung, Wörterbuch, Snippets, Formatierungsprofile
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
- **Keine automatische LLM-Nachbearbeitung im Standardpfad.** Die Refinement-Stufe
  (`refine_enabled`) ist standardmäßig aus und für den stabilen Pfad nicht vorgesehen.

## Unsicherheiten, die noch gemessen werden müssen

- Vulkan gegen CPU auf dieser Hardware. **Es existiert kein publizierter Benchmark** dazu, und
  die gemessenen 392 ms Inferenz auf reiner CPU stellen den praktischen Nutzen einer
  GPU-Beschleunigung für Diktatlängen ohnehin infrage.
- Deutsche Interpunktions- und Großschreibqualität über den einen Testsatz hinaus. Gängige
  WER-Benchmarks entfernen Interpunktion vor der Wertung und sagen darüber nichts aus.
- Verhalten der Injektion in Terminals, Electron-Anwendungen und über RDP.
- Ob die 150-ms-Untergrenze nach dem Einfügen für langsame Zielanwendungen (Word beim
  Kaltstart, Electron) ausreicht, bevor die Zwischenablage zurückgesetzt wird.
