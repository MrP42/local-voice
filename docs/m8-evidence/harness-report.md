# M8 meetings - Abnahme-Harness

Lauf: 2026-08-19 19:46:52 | Szenario-Satz: `all` | Binary: `C:\Users\wolff\local-voice-project\apps\local-voice\scripts\..\src-tauri\target\release\local-voice-ai.exe`

**Automatisiert: 6/6 PASS.** Alles darunter unter "Manuell offen" wurde
NICHT gemessen und ist als offen zu lesen - keine dieser Zeilen ist ein Ergebnis.

Nachstellen:

```powershell
cd apps\local-voice
.\scripts\make-m8-fixtures.ps1        # idempotent, ueberspringt vorhandene Fixtures
.\scripts\m8-verify.ps1               # oder -Scenario <name> fuer einen einzelnen Fall
```

Die Szenarien laufen headless ueber die CLI-Flags `--import-meeting`,
`--dump-meeting` und `--make-orphan`. Jeder Meetings-Lauf macht vorher
dieselbe Startroutine wie die App (`recover_orphans` + `purge_due_audio`);
ein zweiter Lauf ist damit exakt ein App-Neustart.

| Szenario | Ergebnis | Messung |
|---|---|---|
| import-wav | PASS | status=ready, segments=2, channels=[2], chars=901, duration=60000 ms, import=3851 ms |
| import-matrix | PASS | mp4: status=ready, segments=2, chars=901, 3635 ms // vtt: status=ready, segments=3, times=[1000-4500 10000-14250 60000-65000] // stereo-44k1: status=ready, segments=2, duration=60000 ms, chars=901, 3349 ms |
| silence-timeline | PASS | status=ready, segments=9, first_start=0 ms, last_start=550009 ms, last_end=599996 ms, duration=600000 ms, silence gap 181190 ms starting at 183677 ms, import=32664 ms |
| log-privacy | PASS | no spoken word appears in handy.log (5 words probed, transcript 901 chars, log 198,5 KB) |
| retention | PASS | policy=Days(0); before: file=True, until=1787161609, segments=2; after: mic_path=null, until=null, segments=2, chars=901 |
| orphan-recovery | PASS | orphan wav 1920044 bytes; declared data length 0 -> 1920000 (expected 1920000); ffprobe after repair 60,00 s; status=ready, segments=1 |

### import-wav - Rohdaten (ohne Segmenttexte)
```json
{
  "title": "m8_short_de",
  "audio_retention_until": null,
  "source": "import",
  "last_end_ms": 59999,
  "total_text_chars": 901,
  "status": "ready",
  "consent_confirmed_at": 1787161553,
  "ended_at": 1787161557,
  "id": "01M0DJ2ABDDT0N4RMD2V057EQR",
  "channels": [
    {}
  ],
  "system_audio_path": null,
  "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\01M0DJ2ABDDT0N4RMD2V057EQR\\import.wav",
  "last_start_ms": 45412,
  "segment_count": 2,
  "document_kinds": [],
  "duration_ms": 60000,
  "first_start_ms": 0
}
```

### import-matrix - Rohdaten (ohne Segmenttexte)
```json
{
  "stereo": {
    "title": "m8_stereo_44k",
    "audio_retention_until": null,
    "source": "import",
    "last_end_ms": 59999,
    "total_text_chars": 901,
    "status": "ready",
    "consent_confirmed_at": 1787161563,
    "ended_at": 1787161566,
    "id": "01M0DJ2KWW9PBM8RPG1W8YRZG8",
    "channels": [
      {}
    ],
    "system_audio_path": null,
    "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\01M0DJ2KWW9PBM8RPG1W8YRZG8\\import.wav",
    "last_start_ms": 45412,
    "segment_count": 2,
    "document_kinds": [],
    "duration_ms": 60000,
    "first_start_ms": 0
  },
  "mp4": {
    "title": "m8_import",
    "audio_retention_until": null,
    "source": "import",
    "last_end_ms": 60031,
    "total_text_chars": 901,
    "status": "ready",
    "consent_confirmed_at": 1787161558,
    "ended_at": 1787161561,
    "id": "01M0DJ2F4P96572Q9HDHC0A5D8",
    "channels": [
      {}
    ],
    "system_audio_path": null,
    "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\01M0DJ2F4P96572Q9HDHC0A5D8\\import.wav",
    "last_start_ms": 45431,
    "segment_count": 2,
    "document_kinds": [],
    "duration_ms": 60032,
    "first_start_ms": 0
  },
  "vtt": {
    "title": "m8_sub",
    "audio_retention_until": null,
    "source": "subtitle",
    "last_end_ms": 65000,
    "total_text_chars": 172,
    "status": "ready",
    "consent_confirmed_at": 1787161562,
    "ended_at": null,
    "id": "01M0DJ2JZC0JJ8RXHF8HBXGMNH",
    "channels": [
      {}
    ],
    "system_audio_path": null,
    "mic_audio_path": null,
    "last_start_ms": 60000,
    "segment_count": 3,
    "document_kinds": [],
    "duration_ms": null,
    "first_start_ms": 1000
  }
}
```

### silence-timeline - Rohdaten (ohne Segmenttexte)
```json
{
  "title": "m8_silence_gap",
  "audio_retention_until": null,
  "source": "import",
  "last_end_ms": 599996,
  "total_text_chars": 6225,
  "status": "ready",
  "consent_confirmed_at": 1787161567,
  "ended_at": 1787161600,
  "id": "01M0DJ2RAGH9MJKCB9F2GXFH4F",
  "channels": [
    {}
  ],
  "system_audio_path": null,
  "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\01M0DJ2RAGH9MJKCB9F2GXFH4F\\import.wav",
  "last_start_ms": 550009,
  "segment_count": 9,
  "document_kinds": [],
  "duration_ms": 600000,
  "first_start_ms": 0
}
```

### log-privacy - Rohdaten (ohne Segmenttexte)
```json
{
  "title": "m8_short_de",
  "audio_retention_until": null,
  "source": "import",
  "last_end_ms": 59999,
  "total_text_chars": 901,
  "status": "ready",
  "consent_confirmed_at": 1787161601,
  "ended_at": 1787161604,
  "id": "01M0DJ3S58ZVA5T6XWWJSK6AY6",
  "channels": [
    {}
  ],
  "system_audio_path": null,
  "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\01M0DJ3S58ZVA5T6XWWJSK6AY6\\import.wav",
  "last_start_ms": 45412,
  "segment_count": 2,
  "document_kinds": [],
  "duration_ms": 60000,
  "first_start_ms": 0
}
```

### retention - Rohdaten (ohne Segmenttexte)
```json
{
  "before": {
    "import_ms": 3875,
    "audio_retention_until": 1787161609,
    "source": "import",
    "last_end_ms": 59999,
    "source_file": "C:\\Users\\wolff\\local-voice-project\\apps\\local-voice\\scripts\\..\\src-tauri\\tests\\fixtures\\m8_short_de.wav",
    "meeting_id": "01M0DJ3XKC8BXFNF4SB6HZVFQH",
    "consent_confirmed_at": 1787161605,
    "ended_at": 1787161609,
    "status": "ready",
    "id": "01M0DJ3XKC8BXFNF4SB6HZVFQH",
    "channels": [
      {}
    ],
    "system_audio_path": null,
    "db": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\meetings.db",
    "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\lva-m8-sandbox-20260819-194552\\01M0DJ3XKC8BXFNF4SB6HZVFQH\\import.wav",
    "last_start_ms": 45412,
    "segment_count": 2,
    "title": "m8_short_de",
    "total_text_chars": 901,
    "document_kinds": [],
    "audio_file_exists": true,
    "duration_ms": 60000,
    "first_start_ms": 0
  },
  "after": {
    "title": "m8_short_de",
    "audio_retention_until": null,
    "source": "import",
    "last_end_ms": 59999,
    "total_text_chars": 901,
    "status": "ready",
    "consent_confirmed_at": 1787161605,
    "ended_at": 1787161609,
    "id": "01M0DJ3XKC8BXFNF4SB6HZVFQH",
    "channels": [
      {}
    ],
    "system_audio_path": null,
    "mic_audio_path": null,
    "last_start_ms": 45412,
    "segment_count": 2,
    "document_kinds": [],
    "duration_ms": 60000,
    "first_start_ms": 0
  }
}
```

### orphan-recovery - Rohdaten (ohne Segmenttexte)
```json
{
  "title": "Crash-Test",
  "audio_retention_until": null,
  "source": "live",
  "last_end_ms": 3000,
  "total_text_chars": 30,
  "status": "ready",
  "consent_confirmed_at": 0,
  "ended_at": null,
  "id": "01M0DJ43PQTE9KB1BGDB7CMWYE",
  "channels": [
    {}
  ],
  "system_audio_path": null,
  "mic_audio_path": "C:\\Users\\wolff\\AppData\\Local\\Temp\\m8-orphan-DB7CMWYE\\mic.wav",
  "last_start_ms": 0,
  "segment_count": 1,
  "document_kinds": [],
  "duration_ms": null,
  "first_start_ms": 0
}
```

### Zurueckgelassene Test-Meetings

Der Harness laeuft gegen die echte `meetings.db` (dort liegt das installierte
Modell). Diese Meetings bleiben stehen und koennen in der App geloescht werden:

- `01M0DJ2ABDDT0N4RMD2V057EQR`
- `01M0DJ2F4P96572Q9HDHC0A5D8`
- `01M0DJ2JZC0JJ8RXHF8HBXGMNH`
- `01M0DJ2KWW9PBM8RPG1W8YRZG8`
- `01M0DJ2RAGH9MJKCB9F2GXFH4F`
- `01M0DJ3S58ZVA5T6XWWJSK6AY6`
- `01M0DJ3XKC8BXFNF4SB6HZVFQH`
- `01M0DJ43PQTE9KB1BGDB7CMWYE`

- sandbox: Harness lief gegen LVA_MEETINGS_DIR=C:\Users\wolff\AppData\Local\Temp\lva-m8-sandbox-20260819-194552 - die produktive meetings.db wird nie beruehrt; ein Guard bricht sonst hart ab (Exit 10).
- log-privacy: handy.log war beim Test 198,5 KB gross (Rotation bei 500 KB, KeepOne) - geprueft wurde die aktuelle Datei.
- retention: settings_store.json wurde nach dem Lauf aus dem Backup zurueckgeschrieben.
- orphan-recovery: ``--make-orphan`` schreibt erfundene Zeilen in die echte meetings.db und wird im Release-Binary verweigert, solange nicht ``LVA_HARNESS_DESTRUCTIVE=1`` gesetzt ist (Produktivdaten-Integritaet; der Harness laeuft gegen Release). Der Harness setzt die Variable fuer genau diesen einen Aufruf und entfernt sie danach wieder.

## Manuell offen (braucht Patrick am Geraet)

Diese Szenarien brauchen Mikrofon, WASAPI-Loopback, echte Wanduhr-Zeit, die UI
oder einen laufenden Ollama-Server. Sie wurden NICHT gemessen. Fuer jedes steht
unten der Ablauf und das erwartete Ergebnis; die Ist-Spalte bleibt leer, bis
jemand sie ausfuellt.

### M1 Clock-Drift ueber >= 60 min (Spec C2)

1. Referenzdatei bereitlegen: eine Mediendatei mit bekannter Laenge >= 60 min
   (z. B. `ffmpeg -f lavfi -i sine=f=440:d=3600 -ar 48000 ref60.wav`).
2. App starten, Meetings-Bereich, Consent bestaetigen, Systemton-Mitschnitt AN.
3. Aufnahme starten, gleichzeitig `ref60.wav` ueber die Standard-Wiedergabe
   abspielen. Startzeit per Uhr notieren.
4. Nach >= 60 min Aufnahme stoppen, Stoppzeit notieren.
5. Beide WAVs im Meeting-Ordner messen:
   `ffprobe -v error -show_entries format=duration -of csv=p=0 mic.wav`
   dasselbe fuer `system.wav`.

| Groesse | Soll | Ist |
|---|---|---|
| Wanduhr-Dauer | >= 3600 s | |
| `mic.wav` Dauer | Wanduhr +/- 0,5 s je Stunde | |
| `system.wav` Dauer | Wanduhr +/- 0,5 s je Stunde | |
| Differenz mic/system | < 500 ms pro Stunde | |
| Letztes Segment `start_ms` | innerhalb 2 s vor Aufnahmeende | |

Messhilfe fuer das letzte Segment:
`local-voice-ai.exe --dump-meeting <ID> --out drift.json`

### M2 Loopback-Stille (Live-Variante von C1)

1. Aufnahme starten, 3 min sprechen.
2. 3 min NICHTS - kein Mikro, keine Wiedergabe (echte Stille, nicht Mute).
3. 4 min sprechen, stoppen.

| Groesse | Soll | Ist |
|---|---|---|
| Meeting-Status | `ready` | |
| Letztes Segment `start_ms` | > 350000 | |
| `mic.wav` Dauer | ~600 s | |
| Segmente im Stillefenster | keine (oder leerer Text) | |

Der Batch-Zwilling dieses Falls (`silence-timeline`) ist oben automatisiert und
gemessen - was hier fehlt, ist ausschliesslich der Live-Capture-Pfad.

### M3 Crash-Recovery einer LIVEN Aufnahme

Der DB-/WAV-Teil ist oben als `orphan-recovery` automatisiert gemessen. Offen
bleibt der echte Kill mitten im Capture:

1. Aufnahme starten, ca. 2 min sprechen.
2. `Stop-Process -Name local-voice-ai -Force` (kein sauberes Beenden).
3. App neu starten, Meetings oeffnen.

| Groesse | Soll | Ist |
|---|---|---|
| Meeting-Status nach Neustart | `ready` (war `recording`) | |
| `mic.wav` per ffprobe lesbar | ja, ~2 min | |
| Segmente bis zum Kill | vorhanden | |
| Log-Zeile | `meetings: recovered N orphan(s)` | |

### M4 Consent-Gate in der UI (Spec A1)

1. App frisch starten, Meetings oeffnen.
2. Aufnahme starten OHNE die Einwilligung zu bestaetigen.

| Groesse | Soll | Ist |
|---|---|---|
| Fehler | `consent_required` (uebersetzt angezeigt) | |
| Neue Meeting-Zeile | keine | |
| Aufnahmeindikator | bleibt aus | |

Gegenprobe: Einwilligung bestaetigen, starten - Meeting entsteht,
`consent_confirmed_at` ist gesetzt (`--dump-meeting <ID>`).

### M5 Loopback-Hoertest (Qualitaet)

Rein subjektiv, deshalb nicht automatisierbar: eine Videokonferenz mitschneiden
und `system.wav` anhoeren.

| Groesse | Soll | Ist |
|---|---|---|
| Gegenstelle verstaendlich | ja | |
| Aussetzer / Knacken | keine | |
| Lautstaerke | ohne Nachverstaerkung hoerbar | |

### M6 Protokoll mit echtem Ollama

1. Ollama starten, Modell laden.
2. Ein importiertes Meeting oeffnen, Protokoll erzeugen.

| Groesse | Soll | Ist |
|---|---|---|
| Protokoll-Dokument | entsteht, Schema-valide | |
| Redeanteile | aus den Segmenten gerechnet, nicht vom LLM erfunden | |
| Retention `AfterMinutes` | Audio direkt danach geloescht, Pfade genullt | |
| Transkript | unveraendert vorhanden | |

Der Retention-Teil ist oben unter `retention` mit `Days(0)` automatisiert
gemessen; offen ist nur der `AfterMinutes`-Ausloeser ueber ein echtes Protokoll.
