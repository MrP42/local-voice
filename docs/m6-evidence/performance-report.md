# M6 / TP5 „Performance" — Messbericht (2026-08-18)

Ziel: Die TTS-Latenz von „Batch-tauglich" auf „Assistenten-tauglich" bringen.
Zwei Hebel: `torch.compile` am Fish-Server (Windows via triton-windows) und
eine Satz-Pipeline in der App.

## Hebel 1: torch.compile (triton-windows)

| Kennzahl | Eager (vorher) | Compile (nachher) | Faktor |
|---|---|---|---|
| Token-Rate | 3,5 tok/s | **29,5 tok/s** | 8,4× |
| Bandbreite | 16 GB/s | **134,5 GB/s** | 8,4× |
| RTF kurzer Satz (5 s Audio) | ~6 | **0,72** | ~8× |
| RTF langer Text (20 s Audio) | ~6 | **0,63** | ~9× |
| Kompilierzeit (einmalig pro Serverlauf) | — | 2,1 s beim ersten Request |
| Serverstart bis Health | 47–68 s | **109–138 s** | +~60 s |
| VRAM | ~17,3 GB | **~20,6 GB** | +3,3 GB |

Versions-Kopplung (Root Cause eines ersten Fehlschlags): **torch 2.8.0 braucht
triton-windows 3.4.x** — 3.7.1 schlägt mit `ImportError: triton_key` fehl.
Installiert: `triton-windows==3.4.0.post21` ins isolierte Fish-venv.

Zweiter Fehlschlag beim ersten Compile-Lauf: OOM durch **verwaiste
Serverprozesse** — der uv-venv-Launcher spawnt den echten Interpreter als
Kindprozess; nach einem Crash überlebte der Interpreter mit 18,3 GB VRAM.
Diagnose-Regel daraus: vor GPU-Läufen `nvidia-smi` gegenprüfen. Der Kill-Pfad
der App räumt nachweislich beide Prozesse ab (mehrfach verifiziert: 0 Prozesse,
VRAM frei).

## Hebel 2: Satz-Pipeline in der App

HTTP-Streaming des Servers liefert Chunks weiterhin erst am Ende der
Generierung (Event-Loop-Starvation im Fish-Server, auch mit compile). Statt
dagegen anzukämpfen, zerlegt die App Texte an Satzgrenzen
(`protocol::split_sentences`, abkürzungsfest: „z. B." bleibt zusammen) und
spricht pipelined: Satz N spielt, Satz N+1 synthetisiert währenddessen.

| Kennzahl | vorher (eager, ganzer Text) | nachher (compile + Pipeline) |
|---|---|---|
| Zeit bis zum ersten hörbaren Audio | 58,5 s | **~1,8 s** (3 Messläufe: 1,81/1,76/1,74 s) |
| Lückenlose Wiedergabe | — | ja, solange RTF < 1 (gemessen 0,63–0,72) |

## App-Integration (verifiziert)

- Setting `tts_compile` (Default **an**, UI-Toggle mit Messwerten im Text);
  Spawn ergänzt `--compile`.
- Kaltstart-Selbsttest mit compile: Exit 0, ready nach 138 s, Teardown sauber
  (0 Prozesse, VRAM frei).
- `cargo test --lib`: **234 passed** (4 neue: Satz-Splitting ×3, Pipeline-Mock).
- Frontend: Lint 0 Fehler (eigene Dateien), Build Exit 0.

## Konsequenzen für TP3/TP4

Speech-to-Speech-Übersetzung und Stimmwechsler sind mit RTF ~0,65 praktikabel:
Ein 10-s-Ergebnis entsteht in ~7 s, der erste Satz spielt nach ~2 s.
Echtzeit-Stimmwechsler (Mikro→Ohr, <200 ms) bleibt außer Reichweite — dafür
wäre der SGLang-Pfad unter WSL2 der nächste Hebel (dokumentiert, nicht gebaut).
