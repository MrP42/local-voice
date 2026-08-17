#Requires -Version 7.0
<#
.SYNOPSIS
    Scores every fixture through the headless self-test and writes a report.

.DESCRIPTION
    No microphone, no speakers, no window focus — the audio goes straight into
    the transcription pipeline. That makes this repeatable and safe to run while
    someone is using the machine, unlike m3-verify.ps1 which drives the real
    hotkey and types into real windows.

    Use it to compare models, or to see what a settings change did to accuracy
    and streaming latency.

.PARAMETER Model
    Model id. Defaults to the one selected in the app.

.PARAMETER Stream
    Use the live streaming path and report latency instead of batch timing.
#>
[CmdletBinding()]
param(
    [string]$Model,
    [switch]$Stream,
    [string]$AppExe = "$PSScriptRoot\..\src-tauri\target\release\sprechstift.exe",
    [string]$FixtureDir = "$PSScriptRoot\..\src-tauri\tests\fixtures",
    [string]$ArtifactDir = "$PSScriptRoot\..\..\..\docs\m3-evidence"
)

$ErrorActionPreference = 'Stop'

# Reference text per fixture — what the synthesiser was told to say.
#
# These are in SPOKEN form ("dritten Februar", "1.234,50 Euro"), which is what
# the voice actually says. Parakeet normalises numbers on the way out ("3.
# Februar"), so it scores below 100% here while being perfectly correct. That
# gap is the point: it is a real, visible difference between models, and the
# comparison deliberately does not fold number words onto digits to hide it.
# Nemotron, which does not normalise, scores higher against these same texts.
$CASES = @(
    @{ file = 'de_test_01.wav';   ref = 'Guten Tag, dies ist ein Test der lokalen Spracherkennung. Der Termin ist am dritten Februar um vierzehn Uhr dreissig.' }
    @{ file = 'de_umlaute.wav';   ref = 'Der ältere Herr aus der Straße hatte großen Ärger mit seinen Fußballschuhen und trank Glühwein in Köln.' }
    @{ file = 'de_punkt.wav';     ref = 'Kommst du morgen mit? Das wäre wirklich großartig! Ich warte, bis du da bist.' }
    @{ file = 'de_multiline.wav'; ref = 'Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes.' }
    @{ file = 'de_zahlen.wav';    ref = 'Die Rechnung lautet 1.234,50 Euro bei 19 Prozent Mehrwertsteuer.' }
    @{ file = 'de_short_01.wav';  ref = 'Der Termin ist am dritten Februar.' }
)

if (-not (Test-Path $AppExe)) { throw "binary not found: $AppExe" }
New-Item -ItemType Directory -Force -Path $ArtifactDir | Out-Null

$rows = @()
foreach ($case in $CASES) {
    $wav = Join-Path $FixtureDir $case.file
    if (-not (Test-Path $wav)) {
        Write-Host "[SKIP] $($case.file) - fixture missing (see docs/BUILD-WINDOWS.md)" -ForegroundColor DarkGray
        continue
    }

    $resultFile = Join-Path $env:TEMP ("sprechstift-selftest-{0}.json" -f [guid]::NewGuid())

    # Two Windows quirks make this more awkward than it looks:
    #  * the release binary targets the GUI subsystem, so `& exe` returns
    #    immediately and its stdout never reaches this script - hence
    #    Start-Process -Wait and a result FILE rather than a pipe;
    #  * Start-Process does not quote arguments for you, so a reference
    #    sentence would arrive as a dozen separate arguments and clap would
    #    reject the lot with exit code 2.
    $q = '"'
    $cliArgs = @(
        '--transcribe-file', ($q + $wav + $q),
        '--reference', ($q + $case.ref + $q),
        '--json',
        '--out', ($q + $resultFile + $q)
    )
    if ($Model) { $cliArgs += @('--model', ($q + $Model + $q)) }
    if ($Stream) { $cliArgs += '--stream' }

    $proc = Start-Process -FilePath $AppExe -ArgumentList $cliArgs -Wait -PassThru -WindowStyle Hidden
    if (-not (Test-Path -LiteralPath $resultFile)) {
        Write-Host ("[FAIL] {0} - no result (exit {1})" -f $case.file, $proc.ExitCode) -ForegroundColor Red
        $rows += [pscustomobject]@{ Fixture = $case.file; Accuracy = 0; Detail = "exit $($proc.ExitCode)"; Text = '' }
        continue
    }

    $r = (Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json).score
    Remove-Item -LiteralPath $resultFile -Force -ErrorAction SilentlyContinue
    $pct = [math]::Round($r.accuracy * 100, 1)
    $detail = if ($Stream) {
        "first {0}ms, median gap {1}ms" -f $r.first_text_ms, $r.median_gap_ms
    } else {
        "{0} of {1} words" -f $r.correct, $r.reference_words
    }
    $colour = if ($pct -ge 95) { 'Green' } elseif ($pct -ge 80) { 'Yellow' } else { 'Red' }
    Write-Host ("[{0,5:N1}%] {1,-20} {2}" -f $pct, $case.file, $detail) -ForegroundColor $colour

    $rows += [pscustomobject]@{
        Fixture  = $case.file
        Accuracy = $pct
        Detail   = $detail
        Wrong    = $r.substitutions
        Missing  = $r.deletions
        Extra    = $r.insertions
        Text     = $r.recognised
    }
}

$mode = if ($Stream) { 'streaming' } else { 'batch' }
$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
$report = @("# Self-test matrix ($mode) $stamp", '')
if ($Model) { $report += "Model: ``$Model``"; $report += '' }
$report += '| Fixture | Accuracy | Detail | wrong | missing | extra |'
$report += '|---|---|---|---|---|---|'
foreach ($r in $rows) {
    $report += '| {0} | {1}% | {2} | {3} | {4} | {5} |' -f $r.Fixture, $r.Accuracy, $r.Detail, $r.Wrong, $r.Missing, $r.Extra
}
$report += ''
foreach ($r in $rows) {
    if ($r.Text) { $report += "### $($r.Fixture)"; $report += '```'; $report += $r.Text; $report += '```'; $report += '' }
}
$out = Join-Path $ArtifactDir "selftest-$mode.md"
$report -join "`n" | Out-File $out -Encoding UTF8

$mean = if ($rows.Count) { [math]::Round(($rows | Measure-Object Accuracy -Average).Average, 1) } else { 0 }
Write-Host "`nmean accuracy $mean% over $($rows.Count) fixtures" -ForegroundColor Cyan
Write-Host "report: $out"
