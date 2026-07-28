# M2 harness run 2026-07-29 00:11:01

| Scenario | Result | Detail |
|---|---|---|
| normal | PASS | text length 100 |
| umlauts | FAIL | text length 0 |
| punctuation | FAIL | text length 77 |
| multiline | PASS | text length 101 |
| cancel | PASS | notepad should stay empty; got '' |
| silence | PASS | no speech -> '' |
| rapid | PASS | app still alive after 4 rapid toggles: True |
| unfocused | PASS | text went to the focused window instead; app alive: True |

### normal
```
Guten Tag. Dies ist ein Test der lokalen Spracherkennung. Der Termin ist am 3. Februar um 14.30 Uhr.
```

### punctuation
```
Kommst du morgen mit. Das wäre wirklich großartig. Ich warte, bis du da bist.
```

### multiline
```
Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes.
```

### unfocused
```
Guten Tag. Dies ist ein Test der lokalen Spracherkennung. Der Termin ist am 3. Februar um 14.30 Uhr.
```

