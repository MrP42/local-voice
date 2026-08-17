# Nativer Windows-Build und -Test

Reproduzierbarer Ablauf ohne Docker. Alles läuft als natives Windows-Programm.

## Voraussetzungen

| Werkzeug | Version hier | Fundort |
|---|---|---|
| Rust | rustc/cargo 1.97.1 | `%USERPROFILE%\.cargo\bin` |
| Node | v25.8.0 | im PATH |
| PowerShell 7 | 7.6.4 | für `scripts/m3-verify.ps1` (nicht 5.1) |
| Visual Studio 2022 Build Tools | — | für den CMake-Generator |

Bun ist **nicht** installiert; das Frontend wird mit npm gebaut (`node_modules`
liegt im pnpm-Layout vor und ist vollständig).

## Stolperstein 1 — cargo ist nicht im PATH

Weder Git Bash noch PowerShell finden `cargo`. Vor jedem Rust-Befehl:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

Beobachtet 2026-08-17: In Git Bash meldet `cargo test` dann
`bash: cargo: command not found`, und wenn die Ausgabe durch `tail` läuft,
ist der Exit-Code trotzdem **0**. Ein grüner Exit-Code aus einer Pipeline ist
hier also kein Beleg dafür, dass Tests gelaufen sind.

## Stolperstein 2 — CMake-Generator-Konflikt

```
CMake Error: Error: generator : Visual Studio 17 2022
Does not match the generator used previously: Ninja
```

Ursache: `transcribe-cpp-sys` baut über eine kurze NTFS-Junction
`%LOCALAPPDATA%\tcs\<hash>` (MAX_PATH-Umgehung) und legt dort einen
`CMakeCache.txt` an. Ein früherer Lauf hatte diesen Cache mit dem
Ninja-Generator erzeugt.

Der `CMakeCache.txt` liegt **hinter** der Junction, also im echten
`target/<profile>/build/transcribe-cpp-sys-*`-Verzeichnis. Nur den
`tcs`-Ordner zu löschen genügt nicht — er wird neu verlinkt und der alte
Cache ist sofort wieder sichtbar. Beide Seiten müssen weg:

```powershell
Get-ChildItem "apps\local-voice\src-tauri\target\*\build" -Directory `
  -Filter "transcribe-cpp-sys-*" | Remove-Item -Recurse -Force
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\tcs"
```

Danach konfiguriert der Build neu (dauert einmalig ~7 Minuten).

## Ablauf

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# 1) Frontend
cd apps\local-voice
npm run build

# 2) Rust-Tests
cd src-tauri
cargo test --lib          # erwartet: 198 passed, 0 failed

# 3) Release-Binary
cargo build --release     # -> target\release\sprechstift.exe

# 4) Formatprüfung
cargo fmt --check
```

## Native Abnahme

### Sprachfixtures erzeugen

`*.wav` ist per `.gitignore` ausgeschlossen — die Fixtures liegen **nicht** im
Repository und müssen nach einem frischen Checkout erzeugt werden. Der
Generator nutzt die OneCore-Stimme Katja (die alten SAPI-Desktop-Stimmen
sprechen deutsche Umlaute falsch aus, siehe `docs/m2-evidence/VOICE-SETUP.md`):

```powershell
$gen = "apps\local-voice\scripts\bin\TtsGen.exe"
$out = "apps\local-voice\src-tauri\tests\fixtures"
& $gen Katja "$out\de_test_01.wav"   "Guten Tag, dies ist ein Test der lokalen Spracherkennung. Der Termin ist am dritten Februar um vierzehn Uhr dreissig."
& $gen Katja "$out\de_umlaute.wav"   "Ältere Menschen gehen über die Straße in den großen Städten wie Köln."
& $gen Katja "$out\de_punkt.wav"     "Kommst du morgen mit? Das wäre wirklich großartig! Ich warte, bis du da bist."
& $gen Katja "$out\de_multiline.wav" "Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes."
& $gen Katja "$out\de_short_01.wav"  "Der Termin ist am dritten Februar."
& $gen Katja "$out\de_zahlen.wav"    "Die Rechnung lautet 1.234,50 Euro bei 19 Prozent Mehrwertsteuer."
```

`de_short_01.wav` ist absichtlich kurz (rund 2,8 s) — der Dauerlauf spielt sie
100-mal ab.

### Harness starten

Die App muss dafür **laufen**; das Skript steuert sie über den echten Hotkey.

```powershell
# aus dem Repository-Wurzelverzeichnis, mit pwsh (nicht 5.1)
.\apps\local-voice\src-tauri\target\release\sprechstift.exe
pwsh -File apps\local-voice\scripts\m3-verify.ps1
pwsh -File apps\local-voice\scripts\m3-verify.ps1 -Scenario endurance -Runs 100
```

Das Skript liest den Hotkey aus `settings_store.json` — es setzt nicht mehr
Strg+Leertaste voraus. Ergebnisse landen unter `docs/m3-evidence/`.

## GPU (optional)

CPU ist der Standard und für Diktatlängen ausreichend (392 ms für 9,15 s
Audio). Vulkan ist ein Opt-in und braucht das LunarG Vulkan SDK:

```powershell
cargo build --release --features gpu-vulkan
```
