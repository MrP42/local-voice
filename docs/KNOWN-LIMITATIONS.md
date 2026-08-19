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

## Live-Injektion des Streams (stream_injection) — funktioniert, mit einer Lücke

**Der frühere Eintrag „defekt" war überholt und ist am 2026-08-17 durch Messung
widerlegt worden.** Er beschrieb den Stand von Commit `32ee6d3`; beide Ursachen
wurden **danach** behoben, ohne dass jemand nachgemessen hat:

| Symptom | Ursache | behoben in |
|---|---|---|
| `Hi, mein ttttttt ffffffffffff` | `enigo.text()` verliert bei schneller Folge Key-Up-Ereignisse, Windows wiederholt die Taste | `6b9143e` — Fragmente gehen per Ctrl+V raus: zwei Tastenereignisse unabhängig von der Textlänge |
| Kein Text mehr nach der ersten Sprechpause | In transcribe-cpp normalisiert `rebuild_streaming_result_text` Leerzeichen, `token_prefix_raw_bytes` vergleicht dagegen ungetrimmte Token — die Präfixberechnung fror an der ersten Satzgrenze ein | `d223fa8` |

**Verifiziert 2026-08-17** mit Nemotron Streaming 3.5 (Q8_0, SHA-256 gegen den
Katalog geprüft). Der Text wurde jeweils **vor** dem Stopp aus Notepad
zurückgelesen:

| Fixture | Bereits während der Aufnahme im Dokument |
|---|---|
| Normalsatz | „Guten Tag. Dies ist ein Test der lokalen Spracherkennung. Der Termin ist am dritten Februar um vierzehn Uhr dreißig" |
| Drei Absätze mit Pausen | vollständig, alle drei Zeilen |
| Umlaute | „Der ältere Herr aus der Straße hatte großen Ärger mit seinen Fußballschuhen und trank Glühwein in Köln" |

Keine Zeichenwiederholung, keine Duplikate; nach dem Stopp wächst der Text
höchstens um den Rest (etwa den Schlusspunkt) und wird nicht erneut eingefügt.

Die Funktion ist deshalb ein normaler Opt-in-Schalter und nicht mehr zusätzlich
hinter `experimental_enabled` gesperrt.

### Offene Lücke: beim Streaming prüft niemand den Fokus

Der abgesicherte Einfügepfad (`paste_guard`) gilt für den **finalen**
Einfügevorgang. Beim Streaming ist dieser unterdrückt, und die einzelnen
Fragmente gehen über den Injection-Worker ohne Fokus- oder Rechteprüfung per
Ctrl+V hinaus — `RunState::wants_context()` hängt an der Refinement-Stufe, nicht
am Streaming.

Praktische Folge: **Wechselt der Fokus während des Sprechens, landen die
folgenden Fragmente im neuen Fenster.** Kein stiller Verlust — der Text ist
sichtbar, nur am falschen Ort — aber unkontrolliert, und der Grund, warum der
Batch-Pfad die empfohlene Betriebsart bleibt.

### Modellabhängigkeit

Streaming braucht ein Modell mit `capabilities.streaming`. Für Deutsch bietet der
Katalog nur **Nemotron Streaming 3.5** (0,6 B) und **Voxtral Mini 4B Realtime**.
Der stabile Standard Parakeet V3 meldet `supports_streaming: false` und öffnet gar
keinen Stream — dort bleibt der Schalter wirkungslos.

Nemotron normalisiert gesprochene Zahlen **nicht**: „dritten Februar" statt
„3. Februar", „vierzehn Uhr dreißig" statt „14.30 Uhr". Parakeet tut das. Wer
normalisierte Zahlen braucht, verliert sie mit Streaming.

## Datenschutz: Transkripte in Logdateien

Bis 2026-08-17 schrieb STREAMDIAG bei `debug_mode: true` vollständige Transkripte
im Klartext nach `handy.log` — im vorgefundenen Log standen 264 solcher Zeilen mit
Diktatinhalten des Nutzers.

Seither gilt: **Klartext nur in Debug-Builds.** Im Release-Build loggen STREAMDIAG,
der Segmenter, der Abschlusslog der Transkription, das Transkriptionsergebnis und
die Refinement-Ablehnung ausschließlich Längen und Gate-Namen, unabhängig von
`debug_mode`.

**Der erste Anlauf war unvollständig, und das ist der lehrreiche Teil.** Nach der
ersten Runde galt die Sache als erledigt — der Beleg war eine Suche nach den
bekannten Formatzeichenketten im Binary, und die war grün. Der Dauerlauf über 100
Diktate stellte dann 47 vollständige Diktate im Log sicher: `info!("Transcription
result: {}", …)` war beim Quelltext-Audit schlicht nicht aufgefallen, weil das
Suchmuster sie nicht traf.

Konsequenz: Es gibt jetzt ein Testszenario `log-privacy`, das **nach einem echten
Diktat im echten Log** nach den gesprochenen Wörtern sucht. Ein Quelltext-Audit
findet nur, woran man denkt; eine Messung findet auch das Übersehene.

Nicht davon erfasst und weiterhin im Klartext gespeichert — bewusst, weil es die
Kernfunktion ist:

- `history.db` (Verlaufsdatenbank, Transkripte im Klartext)
- die WAV-Aufnahmen unter `%APPDATA%\de.wolffappliedai.localvoiceai\recordings`

Beides ist die vom Nutzer gewollte Verlaufsfunktion und unterliegt der
Aufbewahrungseinstellung, nicht dem Logging.

**Die alte Logdatei wurde nicht automatisch gelöscht.** Wer die Altlast entfernen
will, löscht sie von Hand:
`%LOCALAPPDATA%\de.wolffappliedai.localvoiceai\logs\handy.log`

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
