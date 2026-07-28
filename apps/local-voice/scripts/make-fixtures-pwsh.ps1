<#
.SYNOPSIS
    Generates the German speech fixtures using the modern OneCore voices.
    Requires PowerShell 7 (pwsh).

.DESCRIPTION
    The legacy SAPI "Desktop" voices reachable from Windows PowerShell 5.1 mangle
    German umlauts: Hedda Desktop renders "Straße" as "Strahe", "großen" as
    "Kroan A" and "Köln" as "Khn". That makes them useless for testing a German
    recogniser - the test would be measuring the synthesiser's defects.

    The OneCore voices (Katja, Stefan) pronounce German correctly and sound far
    more natural. They are only reachable through WinRT, which PowerShell 7
    projects properly via the CsWinRT support in .NET.

.PARAMETER Voice
    Substring of the voice name. Default Katja. Use -List to see what is installed.

.EXAMPLE
    pwsh -File make-fixtures-pwsh.ps1 -List
    pwsh -File make-fixtures-pwsh.ps1 -Voice Stefan
#>
[CmdletBinding()]
param(
    [string]$Voice = 'Katja',
    [switch]$List,
    [string]$OutDir = "$PSScriptRoot\..\src-tauri\tests\fixtures"
)

$ErrorActionPreference = 'Stop'

if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "This script needs PowerShell 7 (pwsh). Windows PowerShell 5.1 cannot project the WinRT async API."
}

Add-Type -AssemblyName System.Runtime.WindowsRuntime -ErrorAction SilentlyContinue

# WinRT type projection
$null = [Windows.Media.SpeechSynthesis.SpeechSynthesizer, Windows.Media, ContentType = WindowsRuntime]
$null = [Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType = WindowsRuntime]

function Wait-WinRT {
    param($Op)
    # In PS7 the WinRT IAsyncOperation is projected with a usable Status/GetResults
    # pair; polling avoids needing the GetAwaiter extension methods entirely.
    while ($Op.Status -eq 'Started') { Start-Sleep -Milliseconds 30 }
    if ($Op.Status -ne 'Completed') { throw "WinRT operation ended with status $($Op.Status)" }
    $Op.GetResults()
}

$synth  = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::new()
$voices = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::AllVoices

if ($List) {
    Write-Host "OneCore voices available:" -ForegroundColor Cyan
    $voices | ForEach-Object { "  {0,-28} {1,-8} {2}" -f $_.DisplayName, $_.Language, $_.Gender }
    return
}

$chosen = $voices | Where-Object { $_.DisplayName -like "*$Voice*" } | Select-Object -First 1
if (-not $chosen) {
    Write-Warning "Voice '$Voice' not found. Available:"
    $voices | ForEach-Object { "  $($_.DisplayName)" }
    throw "voice not found"
}
$synth.Voice = $chosen
Write-Host "Using voice: $($chosen.DisplayName) [$($chosen.Language)]" -ForegroundColor Green

# Real German orthography: these voices pronounce ä/ö/ü/ß correctly, so the
# fixtures can finally use proper spelling and the test genuinely exercises
# German phonetics rather than ASCII substitutes.
$fixtures = [ordered]@{
    'de_test_01.wav'   = 'Guten Tag, dies ist ein Test der lokalen Spracherkennung. Der Termin ist am dritten Februar um vierzehn Uhr dreißig.'
    'de_umlaute.wav'   = 'Der ältere Herr aus der Straße hatte großen Ärger mit seinen Fußballschuhen und trank Glühwein in Köln.'
    'de_punkt.wav'     = 'Kommst du morgen mit? Das wäre wirklich großartig! Ich warte, bis du da bist.'
    'de_multiline.wav' = 'Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes.'
    'de_lang.wav'      = 'Wir treffen uns am Montag um neun Uhr im Besprechungsraum drei, um die Ergebnisse der letzten Messreihe durchzugehen. Bitte bringen Sie die Auswertung mit, damit wir die Abweichungen bei den Temperaturwerten gemeinsam prüfen können.'
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

foreach ($name in $fixtures.Keys) {
    $stream = Wait-WinRT $synth.SynthesizeTextToStreamAsync($fixtures[$name])
    $size   = [uint32]$stream.Size
    $reader = [Windows.Storage.Streams.DataReader]::new($stream.GetInputStreamAt(0))
    Wait-WinRT $reader.LoadAsync($size) | Out-Null
    $bytes  = [byte[]]::new($size)
    $reader.ReadBytes($bytes)
    $reader.Dispose()

    $path = Join-Path $OutDir $name
    [System.IO.File]::WriteAllBytes($path, $bytes)
    Write-Host ("  {0,-20} {1,9:N0} bytes" -f $name, $bytes.Length)
}

Write-Host "`nFixtures written to $OutDir" -ForegroundColor Cyan
