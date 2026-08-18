# M5 / TP2 „Stimmen klonen" — Abnahmebericht (2026-08-18)

Spec: `docs/superpowers/specs/2026-08-18-tp2-stimmen-klonen-design.md`
Branch: `feat/m4-tts-vorlesen` (M4+M5 gemeinsam, ein PR-Schnitt)

## Testebenen

| Ebene | Ergebnis |
|---|---|
| `cargo test --lib` | **230 passed, 0 failed** (8 neue: Protokoll-Referenz, Stimm-Id-Sanitizing, Save/List/Delete-Roundtrip, WAV-Loader, Mock „reference_id im Request") |
| ESLint (tts-Komponenten) | **0 Fehler** |
| `pnpm run build` | **Exit 0** |

## Live-Verifikation gegen den echten Fish-Server (RTX 4090)

Referenzstimme „testref" im Server-Format angelegt
(`C:\AI\fish-speech\references\testref\sample.wav` + `sample.lab` — die
Fish-generierte deutsche Testdatei als synthetische Referenz; bewusst kein
biometrisches Material für den automatisierten Test).

| Lauf | Ergebnis |
|---|---|
| `--tts-test --tts-voice testref` (Kaltstart) | Exit 0 · Server-Start 49,6 s · Synthese 46,6 s · 696 364 Bytes |
| `--tts-test --tts-voice testref --tts-out-wav …` | Exit 0 · identische Bytezahl · **hörbares Artefakt** `C:\AI\fish-speech\output\cloned_testref.wav` (Zielsatz: „Hallo Patrick. Das ist deine lokal geklonte Stimme. …") |
| Ohne Stimme (Regression) | 282 668 Bytes — andere Stimme, deterministisch wie zuvor |

Die unterschiedliche Bytezahl mit/ohne `reference_id` bei gleichem Seed belegt,
dass die Referenzstimme den Klang tatsächlich bestimmt (nicht nur mitgesendet
wird); der Mock-Test belegt zusätzlich Feld und `use_memory_cache:"on"` im
Request-Body.

## Abnahmekriterien der Spec

| # | Kriterium | Status |
|---|---|---|
| 1 | Stimme anlegbar (Aufnahme + Import), erscheint in Liste | **implementiert**; FS-Roundtrip unit-verifiziert; UI-Aufnahmeflow manuell offen (Mikro) |
| 2 | Transkript automatisch + editierbar | **implementiert** (STT-Anbindung; UI-Textarea); STT-Live-Lauf manuell offen |
| 3 | Vorlesen nutzt aktive Stimme (`reference_id` nachweisbar) | **verifiziert** (Mock + Live, s. o.) |
| 4 | Löschen räumt Verzeichnis + aktive Auswahl | **verifiziert** (Unit + Setting-Reset-Code) |
| 5 | Referenzdaten bleiben lokal | **verifiziert** — git-excluded (fish-Repo `info/exclude`), keine Uploads, Transkripte nur als Länge geloggt |

## Manuelle Restpunkte (Patrick)

1. Bereich „Vorlesen" → „Stimmen" → **Neue Stimme aufnehmen** (10–30 s) →
   Transkript prüfen → Name „patrick" → Speichern (aktiviert automatisch).
2. Vorlesen-Hotkey drücken → eigene geklonte Stimme hören.
3. Anhören der synthetischen Klon-Evidence: `C:\AI\fish-speech\output\cloned_testref.wav`.

## Einschränkungen

- In-App-Aufnahme ist 16 kHz mono (Diktatpfad) — für maximale Klonqualität
  eine Studio-WAV importieren (bleibt unverändert erhalten, Fish resampled).
- Erste Nutzung einer Stimme pro Server-Sitzung encodiert die Referenz (~+20 s
  bei RTF ~6); danach greift der Server-Cache (`use_memory_cache`).
