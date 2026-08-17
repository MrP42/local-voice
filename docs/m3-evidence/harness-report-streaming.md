# M3 harness run 2026-08-17 21:14:46

Scenario set: `streaming`

| Scenario | Result | Detail |
|---|---|---|
| streaming-normal | PASS | during=114 chars, after=114 chars, early=True, no-duplicate=True |
| streaming-pauses | PASS | during=99 chars, after=99 chars, early=True, no-duplicate=True |
| streaming-umlauts | PASS | during=85 chars, after=85 chars, early=True, no-duplicate=True |

### streaming-normal
```
Guten Tag Dies ist ein Test der lokalen Spracherkennung. Der Termin ist am dritten Februar um vierzehn Uhr dreißig
```

### streaming-pauses
```
Erste Zeile des Textes Neuer Absatz, Zweite Zeile des Textes, Neuer Absatz, dritte Zeile des Textes
```

### streaming-umlauts
```
Der ältere Herr aus der Straße hatte große Närger mit seinen Fußballschuhen und trank
```

