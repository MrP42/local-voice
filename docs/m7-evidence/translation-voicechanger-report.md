# M7 / TP3+TP4 „Audio-Übersetzung + Stimmwechsler" — Abnahmebericht (2026-08-18)

Branch: `feat/m4-tts-vorlesen` · beide Features sind Kompositionen bereits
einzeln verifizierter Bausteine (STT aus M2/M3, TTS/Stimmen/Pipeline aus M4–M6).

## TP3 Audio-Übersetzung

Pipeline: Aufnahme/Text → STT (lokal) → Textübersetzung über den
Post-Process-Provider → Sprachausgabe in der aktiven Stimme (lokal).

| Prüfung | Ergebnis |
|---|---|
| `translator::translate` gegen Mock-LLM (OpenAI-Format) | **PASS** (Antwort durchgereicht, getrimmt) |
| Fehlerpfad ohne konfiguriertes Modell | **PASS** — Meldung nennt den lokalen Weg (Custom → Ollama) |
| Prompt-Regeln (nur Übersetzung, Ton/Namen erhalten) | **PASS** (pure Test) |
| **Live: exakter App-Prompt gegen lokales Ollama** | **PASS** — „Hallo Patrick, die lokale Sprachübersetzung…" → "Hello Patrick, the local language translation is now fully functional on your own machine." (13,3 s inkl. Modell-Load, Modell gemma-4-26B-A4B lokal) |
| UI-Karte (Zielsprache persistiert, Text- + Aufnahme-Flow) | implementiert; Mikro-Flow manuell offen |

**Wichtige Betriebs-Randbedingung:** Übersetzungs-LLM (Ollama) und Fish-TTS
teilen sich die GPU. Das vorhandene 15-GB-Modell + Fish (20,6 GB) überbuchen
die 24 GB → für flüssige Speech-to-Speech-Übersetzung ein **kleines
Übersetzungsmodell** verwenden (z. B. ein 3–4-GB-Modell), das neben Fish passt.
Ollama entlädt Modelle nach Leerlauf selbst.

## TP4 Stimmwechsler

Offline-Kaskade: Aufnahme oder WAV-Datei → STT → Nachsprechen in der aktiven
(geklonten) Stimme; zusätzlich Export als WAV-Datei (Save-Dialog).
Bewusst **kein Echtzeit-Effekt** — dafür wäre der SGLang-Pfad (WSL2) nötig;
mit RTF 0,65 entsteht ein 10-s-Ergebnis in ~7 s.

| Prüfung | Ergebnis |
|---|---|
| WAV-Loader beliebiger Formate (44,1 kHz stereo → 16 k mono) | **PASS** (Unit, Resampling-Längen + Downmix-Pegel) |
| Synthese-in-Datei-Pfad | **PASS** — identischer Code-Pfad wie `--tts-out-wav` (live verifiziert in M5: 696-KB-Klon-WAV) |
| STT-Baustein | in M2/M3 nativ abgenommen (11/11 Szenarien, 100er-Dauerlauf) |
| UI-Karte (Aufnehmen/Datei/Export) | implementiert; Mikro-Flow manuell offen |

## Gesamtstand Tests

`cargo test --lib`: **237 passed, 0 failed** · ESLint (tts-Komponenten): 0 Fehler
· `pnpm run build`: Exit 0 · GUI-Smoke (App-Start mit allen vier Karten): PASS.

## Manuelle Restpunkte (Patrick, ~10 min)

1. „Vorlesen"-Bereich öffnen: Serverstart beobachten, Text vorlesen lassen.
2. Eigene Stimme aufnehmen (Karte „Stimmen"), Probesatz hören.
3. Übersetzung: kleines Ollama-Modell wählen (Einstellungen → Nachbearbeitung →
   Custom + Modellname), Satz einsprechen, englische Ausgabe hören.
4. Stimmwechsler: kurzen Satz aufnehmen → in geklonter Stimme nachgesprochen
   hören → als WAV exportieren.
