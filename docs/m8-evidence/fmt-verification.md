# Maschinelle Verifikation des chore(fmt)-Commits `0b7ce26`

**Auflage aus dem M8-Review (2026-08-19):** Der Formatierungs-Commit
`0b7ce26` („rustfmt reflows in M4-M7 files") war nur stichprobengeprüft;
verlangt war ein maschineller Nachweis, dass keine Semantik enthalten ist.

## Warum `git diff -w` hier nicht beweiskräftig ist

`git diff 065e7c4 0b7ce26 -w --ignore-blank-lines` ist **nicht leer**
(31 326 Bytes). Das ist erwartbar und kein Semantik-Beleg in beide
Richtungen: rustfmt-Reflows verteilen Tokens auf neue Zeilen — für `-w`
sind das Zeilenänderungen, obwohl der Token-Strom identisch ist
(Beispiel: `app.state::<…>().inner().clone().reading_play()` auf vier
Zeilen umgebrochen).

## Beweiskräftiger Test: rustfmt-Normalisierung beider Stände

Für jede der 10 Dateien wurden Parent- und Commit-Stand durch dasselbe
rustfmt geschickt und byteweise verglichen:

```bash
git show 065e7c4:<datei> | rustfmt --edition 2021 --emit stdout > a.rs
git show 0b7ce26:<datei> | rustfmt --edition 2021 --emit stdout > b.rs
cmp a.rs b.rs
```

**Ergebnis (2026-08-19, rustfmt aus stable-x86_64-pc-windows-msvc):**

| Datei | nach rustfmt |
|---|---|
| commands/tts.rs | IDENTISCH |
| managers/meetings/store.rs | IDENTISCH |
| managers/tts/mod.rs | IDENTISCH |
| managers/tts/protocol.rs | IDENTISCH |
| managers/tts/state.rs | IDENTISCH |
| managers/tts/voices.rs | IDENTISCH |
| media.rs | IDENTISCH |
| selftest.rs | IDENTISCH |
| settings.rs | IDENTISCH |
| translator.rs | IDENTISCH |

Beide Stände jeder Datei normalisieren auf denselben Byte-Strom — der
Commit ist damit exakt rustfmt-Output über dem Parent-Stand und enthält
**beweisbar keine Semantikänderung**.
