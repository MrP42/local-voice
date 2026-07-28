<#
.SYNOPSIS
    Generates the German speech fixtures used by the M2 acceptance harness.

.DESCRIPTION
    Voice is selectable. Run with -List to see what this machine offers.

    A note on voice quality, because the default is genuinely unpleasant:
    Windows ships two *generations* of voices.

      * SAPI5 "Desktop" voices (Hedda Desktop, Zira Desktop) - the old, robotic
        ones. These are the only voices reachable from System.Speech and from the
        SAPI.SpVoice COM object, which is what this script uses.
      * OneCore voices (Katja, Stefan, Hedda) - markedly more natural. They live
        under HKLM\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens and are only
        reachable through the WinRT Windows.Media.SpeechSynthesis API.

    Windows PowerShell 5.1 cannot drive the WinRT async API without extra tooling,
    and making the OneCore voices visible to SAPI requires copying registry tokens
    under HKLM, which needs administrator rights and changes the machine for every
    application. That is not something this script does behind your back.

    If you want the better voices for fixtures, either run the generator from
    PowerShell 7 (which projects WinRT properly), or copy the tokens yourself:

        # elevated, and it affects all SAPI applications on the machine
        reg copy "HKLM\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens\MSTTS_V110_deDE_KatjaM" `
                 "HKLM\SOFTWARE\Microsoft\Speech\Voices\Tokens\MSTTS_V110_deDE_KatjaM" /s /f

.PARAMETER Voice
    Substring of the voice name, e.g. 'Hedda'. Default: first German voice found.

.PARAMETER List
    Print available voices and exit.

.EXAMPLE
    .\make-fixtures.ps1 -List
    .\make-fixtures.ps1 -Voice Hedda
#>
[CmdletBinding()]
param(
    [string]$Voice,
    [switch]$List,
    [int]$Rate = -1,
    [string]$OutDir = "$PSScriptRoot\..\src-tauri\tests\fixtures"
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech

$synth  = New-Object System.Speech.Synthesis.SpeechSynthesizer
$voices = $synth.GetInstalledVoices() | Where-Object { $_.Enabled }

if ($List) {
    Write-Host "Voices reachable from SAPI (usable by this script):" -ForegroundColor Cyan
    $voices | ForEach-Object {
        "  {0,-28} {1,-8} {2}" -f $_.VoiceInfo.Name, $_.VoiceInfo.Culture, $_.VoiceInfo.Gender
    }
    $oneCore = 'HKLM:\SOFTWARE\Microsoft\Speech_OneCore\Voices\Tokens'
    if (Test-Path $oneCore) {
        Write-Host "`nOneCore voices present on this machine but NOT reachable here" -ForegroundColor Yellow
        Write-Host "(better quality - see the notes at the top of this script):" -ForegroundColor Yellow
        Get-ChildItem $oneCore | ForEach-Object { "  $((Get-ItemProperty $_.PSPath).'(default)')" }
    }
    return
}

if ($Voice) {
    $chosen = $voices | Where-Object { $_.VoiceInfo.Name -like "*$Voice*" } | Select-Object -First 1
    if (-not $chosen) { throw "Voice '$Voice' not found. Run with -List." }
} else {
    $chosen = $voices | Where-Object { $_.VoiceInfo.Culture.Name -like 'de*' } | Select-Object -First 1
    if (-not $chosen) { throw "No German voice installed. Run with -List." }
}

$synth.SelectVoice($chosen.VoiceInfo.Name)
$synth.Rate = $Rate
Write-Host "Using voice: $($chosen.VoiceInfo.Name) [$($chosen.VoiceInfo.Culture)] rate=$Rate" -ForegroundColor Green

# IMPORTANT - why the source text avoids literal umlauts.
#
# The SAPI "Hedda Desktop" voice mispronounces ä/ö/ü/ß badly: it renders "Straße"
# as "Strahe", "großen" as "Kroan A" and "Koeln" written as "Köln" as "Khn". Feeding
# that to the recogniser tests the synthesiser's defects, not the recogniser.
#
# Written with ae/oe/ue/ss, Hedda pronounces correct German - and the recogniser
# then correctly emits real umlauts (verified: it returns "ältere", "Straße",
# "großen", "Köln"). So the assertions in m2-verify.ps1 deliberately expect the
# proper umlaut characters even though the source text here is transliterated.
# That is the stronger test: correct German audio in, correct German text out.
#
# If you install/expose the OneCore voices (Katja, Stefan) they handle literal
# umlauts properly - see the notes at the top of this file.
$fixtures = [ordered]@{
    'de_test_01.wav'   = 'Guten Tag, dies ist ein Test der lokalen Spracherkennung. Der Termin ist am dritten Februar um vierzehn Uhr dreissig.'
    'de_umlaute.wav'   = 'Der aeltere Herr aus der Strasse hatte grossen Aerger mit seinen Fussballschuhen und trank Gluehwein in Koeln.'
    'de_punkt.wav'     = 'Kommst du morgen mit? Das waere wirklich grossartig! Ich warte, bis du da bist.'
    'de_multiline.wav' = 'Erste Zeile des Textes. Neuer Absatz. Zweite Zeile des Textes. Neuer Absatz. Dritte Zeile des Textes.'
    'de_lang.wav'      = 'Wir treffen uns am Montag um neun Uhr im Besprechungsraum drei, um die Ergebnisse der letzten Messreihe durchzugehen. Bitte bringen Sie die Auswertung mit, damit wir die Abweichungen bei den Temperaturwerten gemeinsam pruefen koennen.'
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
foreach ($name in $fixtures.Keys) {
    $path = Join-Path $OutDir $name
    $synth.SetOutputToWaveFile($path)
    $synth.Speak($fixtures[$name])
    Write-Host ("  {0,-20} {1,8:N0} bytes" -f $name, (Get-Item $path).Length)
}
$synth.SetOutputToDefaultAudioDevice()
$synth.Dispose()
Write-Host "`nFixtures written to $OutDir" -ForegroundColor Cyan
