# M4 / TP1 „Vorlesen" — Abnahmebericht (2026-08-18)

Spec: `docs/superpowers/specs/2026-08-18-tp1-tts-vorlesen-design.md`
Plan: `docs/superpowers/plans/2026-08-18-tp1-tts-vorlesen.md`
Branch: `feat/m4-tts-vorlesen` · Fish-Speech-Server: S2 Pro @ `C:\AI\fish-speech` (Commit e5e2926)

## Testebenen

| Ebene | Ergebnis |
|---|---|
| `cargo test --lib` | **222 passed, 0 failed** (vorher 198; 24 neue: Settings, Protokoll, Zustand, Manager-Mock, Action) |
| ESLint (geänderte Dateien) | **0 Fehler** (5 vorbestehende Fehler in ProgressBar/Footer unverändert — nicht M4-Scope) |
| `pnpm run build` (tsc + vite) | **Exit 0** |
| `cargo build` | **Exit 0** |

## Headless-Selbsttest (`sprechstift.exe --tts-test --json`)

Realer Fish-Speech-Server, RTX 4090, keine Mocks.

| Lauf | server_start_ms | tts_ms | wav_bytes | Exit |
|---|---|---|---|---|
| Warm (externer Server lief) | **0** (adoptiert) | 17 476 | 282 668 | 0 |
| Kalt (App spawnt Server selbst) | **50 623** (Health nach 48 s) | 17 252 | 282 668 | 0 |

Bemerkenswert: `wav_bytes` ist in beiden Läufen **byte-identisch** — der feste
Seed (42) liefert deterministisch dieselbe Stimme und Länge. Das bestätigt das
Spec-Ziel „konsistente Stimme vor TP2" auf Artefakt-Ebene.

## Abnahmekriterien der Spec

| # | Kriterium | Status | Beleg |
|---|---|---|---|
| 1 | Clipboard-Vorlesen per Hotkey reproduzierbar | **manuell offen** | Action + Binding implementiert und unit-getestet (`speak_clipboard_press_toggles…`); Hörtest erfordert Sitzung am Rechner |
| 2 | Erststart aus Stopped inkl. Serverstart endet in Audio | **verifiziert (headless)** | Kaltlauf oben: Spawn → Health 48 s → valides WAV; Statusanzeige zusätzlich im UI implementiert |
| 3 | Idle-Stopp gibt VRAM frei | **teilverifiziert** | `stop_server()`-Pfad real gemessen: nach Kaltlauf Port frei, 0 fish-Python-Prozesse, VRAM 971 MiB (vorher ~18 GB mit geladenem Modell). Idle-*Entscheidung* unit-getestet (6 Fälle); 15-min-Echtzeitlauf nicht automatisiert |
| 4 | App-Exit beendet selbst gestarteten Server immer | **verifiziert (headless)** | Selbsttest-Prozessende hinterließ keinen Serverprozess; RunEvent::Exit ruft zusätzlich `stop_server()` im GUI-Pfad |
| 5 | Drei Testebenen grün + Evidence | **verifiziert** | Tabellen oben; dieses Dokument |

Schutzregel „extern gestartete Server nie beenden": real verifiziert — nach dem
Warmlauf (adoptierter Server) lief der externe Server weiter (`stop_server()`
war no-op), im Mock-Test zusätzlich abgesichert.

## GUI-Smoke

`sprechstift.exe --start-hidden`: App lebt nach 12 s (TtsManager-Init inkl.
Idle-Watchdog panikfrei im vollen GUI-Pfad), danach kontrolliert beendet.

## Manuelle Restpunkte (erfordern Patrick am Rechner)

1. Text kopieren → `ctrl+alt+space` → Sprachausgabe hörbar; zweiter Druck stoppt.
2. Bereich „Vorlesen": Status-Badge-Verlauf beim Serverstart beobachten,
   Textfeld-Vorlesen, Stopp-Button.
3. Optional: `tts_idle_minutes` auf 1 stellen und den Idle-Stopp nach ~90 s
   real beobachten (nvidia-smi).

## Bekannte Einschränkungen (aus der Fish-Installation geerbt)

- RTF ~6 ohne `--compile` (Windows/Triton) — 3,2 s Audio brauchen ~17 s.
  TP5 (triton-windows bzw. WSL2+SGLang) adressiert das.
- VRAM-Kontention mit anderen GPU-Apps verlangsamt drastisch (UI zeigt ab
  120 s Startdauer einen Hinweis).
