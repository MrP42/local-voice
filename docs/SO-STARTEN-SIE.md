# Sprechstift starten und benutzen

Kurzanleitung für den stabilisierten Stand vom 17.08.2026.

## Starten

Doppelklick auf:

```
C:\Users\wolff\local-voice-project\apps\local-voice\src-tauri\target\release\sprechstift.exe
```

Das Fenster erscheint, danach genügt das Symbol im Infobereich. **Das Fenster darf
geschlossen werden** — die Anwendung läuft im Infobereich weiter. Sie muss laufen,
sonst reagiert der Hotkey nicht.

## Diktieren

| Taste | Wirkung |
|---|---|
| **Strg-links + Windows-links** | Aufnahme starten, nochmal drücken zum Beenden |
| **Esc** | laufende Aufnahme abbrechen (kein Text) |

Ablauf: Hotkey drücken → sprechen → Hotkey drücken. Nach rund zwei Sekunden steht der
Text an der Einfügemarke des Fensters, in dem Sie den Hotkey zum Beenden gedrückt haben.

Das Modell ist **Parakeet TDT 0.6B v3** und läuft vollständig lokal auf der CPU. Kein
Netzwerk, kein Docker, kein Ollama, kein Konto.

## Wenn der Text nicht erscheint

Dann erscheint stattdessen für neun Sekunden eine Meldung am Bildschirmrand — mit dem
Grund und dem Hinweis, was zu tun ist. In aller Regel lautet er:

> Der vollständige Text liegt in der Zwischenablage — mit Strg+V einfügen.

Das ist die Zusage dieses Stands: **entweder der Text wird eingefügt, oder er liegt
vollständig in der Zwischenablage und Sie werden darauf hingewiesen.** Verloren geht er
nicht. Zusätzlich steht jedes Diktat im Verlauf im Hauptfenster.

## Was nicht funktioniert

**In Fenstern mit Administratorrechten reagiert der Hotkey nicht.** Windows liefert
Tastendrücke nicht an ein Programm niedrigerer Rechtestufe aus, solange ein solches
Fenster im Vordergrund ist (z. B. der Task-Manager). Es passiert dann schlicht nichts —
die Aufnahmeanzeige bleibt aus, und Sie merken es. Wer dort diktieren will, muss
Sprechstift selbst als Administrator starten.

Wechselt der Fokus erst **nach** dem Beenden der Aufnahme in ein solches Fenster,
greift der oben beschriebene Weg über die Zwischenablage.

**In Fenstern ohne Eingabefeld** (etwa dem Explorer) meldet Windows uns keinen Fehler.
Die Anwendung hält den Einfügeversuch dann für erfolgreich, der Text landet aber
nirgends. Er steht weiterhin im Verlauf.

## Live-Einfügung während des Sprechens (Streaming)

Der Text erscheint dabei schon **während** Sie sprechen, nicht erst nach dem
Beenden. Verifiziert am 17.08.2026 — vollständige Sätze, Sprechpausen und Umlaute
kamen korrekt an, ohne Zeichenwiederholung und ohne Duplikate.

Dafür sind zwei Dinge nötig:

1. **Ein streaming-fähiges Modell.** Für Deutsch ist das **Nemotron Streaming
   3.5**; es ist heruntergeladen und geprüft. Parakeet V3 kann kein Streaming und
   ignoriert den Schalter.
2. **Die Einstellung „Live-Einfügung"** (`stream_injection`) einschalten.

**Zwei Dinge, die Sie dabei wissen sollten:**

- **Der Fokus wird während des Streamings nicht überwacht.** Wechseln Sie
  mitten im Sprechen das Fenster, schreiben die folgenden Bruchstücke in das
  neue Fenster. Der Schutzmechanismus des Standardpfads greift hier nicht.
- **Nemotron normalisiert Zahlen nicht.** Sie erhalten „dritten Februar um
  vierzehn Uhr dreißig" statt „3. Februar um 14.30 Uhr". Wer die normalisierte
  Form braucht, bleibt bei Parakeet V3 ohne Streaming.

Umschalten der Modelle im Hauptfenster unter „Modelle".

## Einstellungen, die bewusst so stehen

| Einstellung | Wert | Grund |
|---|---|---|
| Modell | Parakeet V3 | verifiziert, rund 23-fache Echtzeit, normalisiert Zahlen |
| Live-Einfügung während des Sprechens | aus | funktioniert, aber ohne Fokusüberwachung — siehe oben |
| KI-Nachbearbeitung (Ollama) | aus | gehört nicht in den stabilen Pfad |
| Debug-Modus | aus | — |

## Falls etwas klemmt

Protokoll (enthält **keine** Diktatinhalte):

```
%LOCALAPPDATA%\de.wolffappliedai.sprechstift\logs\handy.log
```

Die Sicherung Ihrer vorherigen Einstellungen liegt unter
`%APPDATA%\de.wolffappliedai.sprechstift\settings_store.json.vor-stabilisierung-2026-08-17.bak`.

Die alte Protokolldatei von vor der Bereinigung enthält noch vollständige Diktate im
Klartext und heißt `handy.log.vor-m3-2026-08-17`. Sie darf gelöscht werden (Issue #6).
