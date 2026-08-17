# M3 harness run 2026-08-17 21:19:49

Scenario set: `all`

| Scenario | Result | Detail |
|---|---|---|
| normal | PASS | 100 chars in 1,9s |
| umlauts | PASS | 103 chars in 1,9s |
| punctuation | PASS | 77 chars in 1,9s |
| multiline | PASS | 101 chars in 1,9s |
| numbers | FAIL | 0 chars in 30,0s; clipboard=1 chars |
| cancel | PASS | notepad must stay empty; got '' |
| silence | PASS | no speech -> '' |
| rapid | PASS | app alive after 4 rapid toggles: True; text '' |
| focus-change | PASS | first='' second=29 chars, clipboard=1 chars |
| no-edit-field | PASS | app alive: True; clipboard '' |
| elevated | FAIL | elevated target: transcript in clipboard: False |
| log-privacy | PASS | no dictated word appears in the log |

### normal
```
Guten Tag. Dies ist ein Test der lokalen Spracherkennung. Der Termin ist am 3. Februar um 14.30 Uhr.
```

### umlauts
```
Der ältere Herr aus der Straße hatte großen Ärger mit seinen Fußballschuhen und trank Glühwein in Köln.
```

### punctuation
```
Kommst du morgen mit? Das wäre wirklich großartig. Ich warte, bis du da bist.
```

### multiline
```
Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes.
```

### focus-change
```
Der Termin ist am 3. Februar.
```

### elevated
```
MARKER-BEFORE
```

