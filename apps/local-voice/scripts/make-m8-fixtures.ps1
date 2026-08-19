#Requires -Version 5.1
<#
.SYNOPSIS
    Generates the M8 meetings acceptance fixtures (German speech, silence
    timeline, import matrix, resample case).

.DESCRIPTION
    ASCII only on purpose - PowerShell 5.1 chokes on em dashes in .ps1 files
    (project rule). German umlauts appear only inside the spoken sentences,
    which are written to disk as UTF-8 and handed to TtsGen.exe; the WinRT
    OneCore voices pronounce them correctly (unlike the SAPI desktop voices,
    see make-fixtures.ps1).

    Idempotent: an existing fixture is left alone unless -Force is passed.

    Fixtures written to -OutDir (default src-tauri/tests/fixtures):

      m8_short_de.wav    60 s German speech, 16 kHz mono 16-bit.
                         Baseline import + log-privacy probe.
      m8_silence_gap.wav 10 min: 3 min speech, 3 min real silence, 4 min
                         speech. The C1 silence-timeline fixture - the last
                         segment must land past 350 s or the batch path is
                         compressing the timeline.
      m8_import.mp4      m8_short_de.wav muxed to AAC/MP4 (import matrix).
      m8_stereo_44k.wav  m8_short_de.wav at 44.1 kHz STEREO. Forces the
                         downmix + resample branch of read_wav_i16_mono_16k
                         (deferred review item from Task 9).
      m8_sub.vtt         3 hand-written cues with known times.

.PARAMETER Voice
    Substring of the WinRT voice name. Default: Katja.

.PARAMETER Force
    Regenerate fixtures that already exist.
#>
[CmdletBinding()]
param(
    [string]$Voice = 'Katja',
    [switch]$Force,
    [string]$OutDir  = "$PSScriptRoot\..\src-tauri\tests\fixtures",
    [string]$TtsGen  = "$PSScriptRoot\bin\TtsGen.exe"
)

$ErrorActionPreference = 'Stop'

function Test-Tool {
    param([string]$Name)
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $cmd) { throw "$Name not found on PATH. Install it (winget install ffmpeg) and retry." }
    return $cmd.Source
}

$ffmpeg = Test-Tool 'ffmpeg'
if (-not (Test-Path $TtsGen)) { throw "TtsGen.exe missing: $TtsGen (see scripts/make-fixtures.ps1)" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

$tmp = Join-Path $env:TEMP ("m8-fixtures-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

# The spoken text. Distinctive nouns on purpose: the log-privacy scenario
# greps the app log for these exact words, so they must not be words the
# logger would print for unrelated reasons ("Modell", "Datei", "Start").
$SENTENCES = @(
    'Guten Morgen, wir beginnen die Sitzung mit dem Quartalsbericht der Fertigung.',
    'Frau Bergmann uebernimmt die Rueckstellung fuer das Projekt Nordlicht bis Freitag.',
    'Der Lieferant hat die Zeitachse um vierzehn Tage verschoben, das betrifft die Montagehalle.',
    'Wir halten fest: das Budget bleibt unveraendert, die Abnahme erfolgt im September.',
    'Herr Kettler prueft die Schnittstelle zum Warenwirtschaftssystem und meldet sich naechste Woche.'
)
# These are what m8-verify.ps1 looks for in the log. Kept here so fixture and
# assertion cannot drift apart.
$PRIVACY_WORDS = @('Quartalsbericht', 'Rueckstellung', 'Nordlicht', 'Montagehalle', 'Kettler')

function Invoke-Ffmpeg {
    param([string[]]$FfArgs, [string]$What)
    $out = & $ffmpeg -hide_banner -loglevel error -y @FfArgs 2>&1
    if ($LASTEXITCODE -ne 0) { throw "ffmpeg failed ($What): $out" }
}

function Get-DurationSeconds {
    param([string]$Path)
    $d = & ffprobe -v error -show_entries format=duration -of csv=p=0 -- "$Path"
    return [double]$d
}

function New-Fixture {
    param([string]$Name, [scriptblock]$Build)
    $path = Join-Path $OutDir $Name
    if ((Test-Path $path) -and -not $Force) {
        Write-Host ("[skip] {0} (exists, use -Force)" -f $Name) -ForegroundColor DarkGray
        return $path
    }
    & $Build $path
    $size = [math]::Round((Get-Item $path).Length / 1MB, 2)
    Write-Host ("[ok]   {0}  {1} MB" -f $Name, $size) -ForegroundColor Green
    return $path
}

# ---------------------------------------------------------------- speech block
# One concatenated block of all sentences, 16 kHz mono - the raw material every
# speech fixture below is looped from.
$block = Join-Path $tmp 'block.wav'
Write-Host "Synthesising $($SENTENCES.Count) sentences with voice '$Voice' ..." -ForegroundColor Cyan
$parts = @()
for ($i = 0; $i -lt $SENTENCES.Count; $i++) {
    $part = Join-Path $tmp ("s{0:D2}.wav" -f $i)
    & $TtsGen $Voice $part $SENTENCES[$i] | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $part)) { throw "TtsGen failed on sentence $i" }
    $parts += $part
}
$listFile = Join-Path $tmp 'concat.txt'
($parts | ForEach-Object { "file '" + ($_ -replace '\\', '/') + "'" }) |
    Set-Content -Path $listFile -Encoding ASCII
Invoke-Ffmpeg @('-f', 'concat', '-safe', '0', '-i', $listFile,
                '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $block) 'speech block'
$blockSecs = Get-DurationSeconds $block
Write-Host ("Speech block: {0:N1} s" -f $blockSecs) -ForegroundColor DarkGray

# Loops $block to exactly $Seconds of speech (no trailing silence padding, so
# the tail is real audio and a truncated timeline would be obvious).
function New-SpeechOfLength {
    param([string]$Target, [int]$Seconds)
    $loops = [math]::Ceiling($Seconds / $blockSecs)
    Invoke-Ffmpeg @('-stream_loop', "$loops", '-i', $block, '-t', "$Seconds",
                    '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $Target) "speech $Seconds s"
}

# ---------------------------------------------------------------- 1. 60 s base
New-Fixture 'm8_short_de.wav' { param($p) New-SpeechOfLength $p 60 } | Out-Null
$shortWav = Join-Path $OutDir 'm8_short_de.wav'

# ---------------------------------------------------------------- 2. C1 silence
New-Fixture 'm8_silence_gap.wav' {
    param($p)
    $sp3 = Join-Path $tmp 'sp180.wav'
    $sp4 = Join-Path $tmp 'sp240.wav'
    $sil = Join-Path $tmp 'sil180.wav'
    New-SpeechOfLength $sp3 180
    New-SpeechOfLength $sp4 240
    # Real digital silence, not a fade: the point of C1 is that a silent
    # stretch still consumes wall-clock time on the transcript timeline.
    Invoke-Ffmpeg @('-f', 'lavfi', '-i', 'anullsrc=r=16000:cl=mono', '-t', '180',
                    '-c:a', 'pcm_s16le', $sil) 'silence 180 s'
    $l = Join-Path $tmp 'gap.txt'
    (@($sp3, $sil, $sp4) | ForEach-Object { "file '" + ($_ -replace '\\', '/') + "'" }) |
        Set-Content -Path $l -Encoding ASCII
    Invoke-Ffmpeg @('-f', 'concat', '-safe', '0', '-i', $l,
                    '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', $p) 'silence gap'
} | Out-Null

# ---------------------------------------------------------------- 3. mp4 mux
New-Fixture 'm8_import.mp4' {
    param($p)
    Invoke-Ffmpeg @('-i', $shortWav, '-c:a', 'aac', '-b:a', '128k', $p) 'mp4 mux'
} | Out-Null

# ---------------------------------------------------------------- 4. resample
New-Fixture 'm8_stereo_44k.wav' {
    param($p)
    Invoke-Ffmpeg @('-i', $shortWav, '-ar', '44100', '-ac', '2', '-c:a', 'pcm_s16le', $p) 'stereo 44k1'
} | Out-Null

# ---------------------------------------------------------------- 5. subtitles
New-Fixture 'm8_sub.vtt' {
    param($p)
    # Exactly three cues with times the harness asserts on. Written LF-only
    # and UTF-8 without BOM: the parser reads plain text, a BOM would end up
    # in the first cue's payload.
    $vtt = @(
        'WEBVTT',
        '',
        '00:00:01.000 --> 00:00:04.500',
        'Guten Morgen, wir beginnen mit dem Quartalsbericht.',
        '',
        '00:00:10.000 --> 00:00:14.250',
        'Frau Bergmann uebernimmt die Rueckstellung fuer Projekt Nordlicht.',
        '',
        '00:01:00.000 --> 00:01:05.000',
        'Herr Kettler prueft die Schnittstelle zur Montagehalle.',
        ''
    ) -join "`n"
    [System.IO.File]::WriteAllText($p, $vtt, (New-Object System.Text.UTF8Encoding($false)))
} | Out-Null

# ---------------------------------------------------------------- manifest
# The harness reads this instead of hard-coding the privacy words and cue
# times, so changing a sentence here cannot silently weaken an assertion.
$manifest = [ordered]@{
    generated_at   = (Get-Date -Format 'o')
    voice          = $Voice
    sentences      = $SENTENCES
    privacy_words  = $PRIVACY_WORDS
    vtt_cues_ms    = @(
        @{ start_ms = 1000;  end_ms = 4500  },
        @{ start_ms = 10000; end_ms = 14250 },
        @{ start_ms = 60000; end_ms = 65000 }
    )
}
$manifestPath = Join-Path $OutDir 'm8_fixtures.json'
$manifest | ConvertTo-Json -Depth 5 | Set-Content -Path $manifestPath -Encoding UTF8
Write-Host "[ok]   m8_fixtures.json" -ForegroundColor Green

Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue

Write-Host "`nFixtures in $OutDir" -ForegroundColor Cyan
Get-ChildItem $OutDir -Filter 'm8_*' | ForEach-Object {
    "  {0,-22} {1,8:N2} MB" -f $_.Name, ($_.Length / 1MB)
}
