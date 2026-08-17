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

## Stolperstein 3 — `cargo build --release` erzeugt kein lauffähiges Produkt

**Das ist der teuerste Fallstrick des Projekts.** Er sieht wie ein Erfolg aus:
`cargo build --release` endet mit Exit 0, die EXE entsteht, sie startet, das
Tray-Symbol erscheint — und die Anwendung ist trotzdem funktionsunfähig.

Beobachtet 2026-08-17: Das Fenster zeigte nur
`localhost – Netzwerkfehler`. Die Webview lud `http://localhost:1420`,
also den **Entwicklungsserver**, statt des eingebetteten Frontends. Ohne
Frontend ruft niemand `initialize_shortcuts` auf (siehe `lib.rs`: Shortcuts
werden bewusst vom Frontend registriert, nicht beim Backend-Start) — der
globale Hotkey ist damit tot, und die gesamte Diktatstrecke reagiert auf
nichts.

Nachweis am Artefakt:

```powershell
$t = [System.Text.Encoding]::ASCII.GetString(
       [System.IO.File]::ReadAllBytes("src-tauri\target\release\sprechstift.exe"))
$t.Contains("localhost:1420")     # darf NICHT True sein
$t.Contains("index-<hash>.js")    # ein Asset aus dist\assets\ - muss True sein
```

Ursache: Das `dev`-Flag setzt `tauri-build` in `build.rs` — und dessen
Ergebnis wird von cargo gecacht. Ein Cache aus einer früheren
`tauri dev`-Sitzung überlebt beliebig viele `cargo build --release`-Läufe,
weil `build.rs` nicht neu ausgeführt wird. Hier stammte er vom 28./29.07.

**Deshalb ist die Tauri-CLI der einzige gültige Build-Weg**; sie setzt die
Umgebungsvariablen korrekt und baut das Frontend vorher mit. Bei Verdacht
zusätzlich den Cache löschen:

```powershell
Get-ChildItem "src-tauri\target\release\build" -Directory -Filter "sprechstift-*" |
  Remove-Item -Recurse -Force
```

`cargo build --release` bleibt für einen reinen **Kompilierbarkeitstest**
brauchbar. Als Beleg dafür, dass die Anwendung funktioniert, ist er wertlos.

## Ablauf

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cd apps\local-voice

# 1) Rust-Tests (Frontend nicht nötig)
cd src-tauri
cargo test --lib          # erwartet: 198 passed, 0 failed
cargo fmt --check
cd ..

# 2) Lauffähiges Release-Binary - NICHT cargo build, siehe Stolperstein 3.
#    Baut das Frontend selbst (beforeBuildCommand) und bettet es ein.
npx tauri build --no-bundle   # -> src-tauri\target\release\sprechstift.exe
```

`--no-bundle` überspringt den Installer; für ein Auslieferungspaket entfällt
der Schalter (siehe Issue #7).

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
