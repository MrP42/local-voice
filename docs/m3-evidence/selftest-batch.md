# Self-test matrix (batch) 2026-08-17 23:32:44

Model: `parakeet-tdt-0.6b-v3`

| Fixture | Accuracy | Detail | wrong | missing | extra |
|---|---|---|---|---|---|
| de_test_01.wav | 84,2% | 16 of 19 words | 2 | 1 | 0 |
| de_umlaute.wav | 100% | 17 of 17 words | 0 | 0 | 0 |
| de_punkt.wav | 100% | 14 of 14 words | 0 | 0 | 0 |
| de_multiline.wav | 100% | 16 of 16 words | 0 | 0 | 0 |
| de_zahlen.wav | 77,8% | 7 of 9 words | 1 | 1 | 0 |
| de_short_01.wav | 83,3% | 5 of 6 words | 1 | 0 | 0 |

### de_test_01.wav
```
Guten Tag, dies ist ein Test der lokalen Spracherkennung. Der Termin ist am 3. Februar um 14.30 Uhr.
```

### de_umlaute.wav
```
Der ältere Herr aus der Straße hatte großen Ärger mit seinen Fußballschuhen und trank Glühwein in Köln.
```

### de_punkt.wav
```
Kommst du morgen mit? Das wäre wirklich großartig. Ich warte, bis du da bist.
```

### de_multiline.wav
```
Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes.
```

### de_zahlen.wav
```
Die Rechnung lautet 1234,50 Euro bei 19% Mehrwertsteuer.
```

### de_short_01.wav
```
Der Termin ist am 3. Februar.
```

