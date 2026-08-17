# M3 harness run 2026-08-17 19:26:10

Scenario set: `all`

| Scenario | Result | Detail |
|---|---|---|
| normal | PASS | 100 chars in 2,0s |
| umlauts | PASS | 103 chars in 1,9s |
| punctuation | PASS | 77 chars in 1,9s |
| multiline | PASS | 101 chars in 1,9s |
| numbers | PASS | 56 chars in 1,9s |
| cancel | PASS | notepad must stay empty; got '' |
| silence | PASS | no speech -> '' |
| rapid | PASS | app alive after 4 rapid toggles: True; text '' |
| focus-change | PASS | first='' second=34 chars, clipboard=42 chars |
| no-edit-field | PASS | app alive: True; clipboard '' |
| elevated | FAIL | target 'Taskmgr': transcript in clipboard: False |

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

### numbers
```
Die Rechnung lautet 1234,50 Euro bei 19% Mehrwertsteuer.
```

### focus-change
```
Der Termin ist am dritten Februar.
```

