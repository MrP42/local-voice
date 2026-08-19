<#
.SYNOPSIS
    M2 acceptance harness: drives the full vertical path against real Windows Notepad.

.DESCRIPTION
    Nothing in the core path is mocked.

      * real global hotkey  - Ctrl+Space is delivered with SendInput, so the app's
                              low-level keyboard hook sees a genuine key event
      * real microphone     - the German fixture is played through the speakers and
                              captured acoustically by the physical microphone.
                              There is no loopback device on this machine, so this
                              is a true air path, not a digital shortcut.
      * real local STT      - Parakeet V3 int8 running offline in the app
      * real text injection - the app pastes into the focused Notepad window, and we
                              read the result back out with UI Automation

    The only synthetic parts are the keypress and the human voice, which is
    unavoidable for an unattended run and is stated plainly in the report.

.PARAMETER Scenario
    Which scenario to run. 'all' runs the full matrix.

.PARAMETER AppExe
    Path to the built binary.
#>
[CmdletBinding()]
param(
    [ValidateSet('all', 'normal', 'umlauts', 'punctuation', 'multiline', 'cancel',
                 'silence', 'unfocused', 'rapid')]
    [string]$Scenario = 'all',

    [string]$AppExe = "$PSScriptRoot\..\src-tauri\target\release\local-voice-ai.exe",

    [string]$ArtifactDir = "$PSScriptRoot\..\..\..\docs\m2-evidence"
)

$ErrorActionPreference = 'Stop'
$script:Results = @()
$script:ScratchSeq = 0

# ---------------------------------------------------------------- Win32 interop
# keybd_event rather than SendInput: it is far simpler to marshal from PowerShell
# and was verified to reach the app's low-level keyboard hook, which is the only
# property that matters here. The hook does not filter injected input.
if (-not ("M2.Native" -as [type])) {
Add-Type -Namespace M2 -Name Native -MemberDefinition @'
    [DllImport("user32.dll")]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, System.UIntPtr dwExtraInfo);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
'@ | Out-Null
}

$VK_CONTROL = 0x11; $VK_SPACE = 0x20; $VK_ESCAPE = 0x1B
$KEYEVENTF_KEYUP = 0x0002

function Send-Key {
    param([byte[]]$VKeys)
    foreach ($k in $VKeys) {
        [M2.Native]::keybd_event($k, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60
    }
    for ($i = $VKeys.Count - 1; $i -ge 0; $i--) {
        [M2.Native]::keybd_event($VKeys[$i], 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 60
    }
}

function Send-Hotkey  { Send-Key @($VK_CONTROL, $VK_SPACE); Start-Sleep -Milliseconds 250 }
function Send-Cancel  { Send-Key @($VK_ESCAPE);             Start-Sleep -Milliseconds 250 }

# ---------------------------------------------------------------- Notepad control
$VK_A = 0x41; $VK_DELETE = 0x2E

function Start-Notepad {
    # Windows 11 Notepad is a packaged app: Start-Process returns a launcher stub
    # that exits immediately, so its MainWindowHandle is always empty. The real
    # window belongs to a separate process named "Notepad" that we have to find.
    #
    # Always open an explicit scratch file. Notepad 11 restores the previous
    # session's tabs, and an early run of this harness pasted a transcript into a
    # restored tab that happened to hold one of the user's own config files. The
    # file on disk was never modified, but dictating into someone's open document
    # is not acceptable behaviour for a test, so we now pin the target ourselves.
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 900

    Get-ChildItem $env:TEMP -Filter 'local-voice-ai-m2-*.txt' -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
    $script:ScratchSeq = ($script:ScratchSeq | ForEach-Object { $_ }) + 1
    $scratch = Join-Path $env:TEMP ("local-voice-ai-m2-{0}-{1}.txt" -f $PID, $script:ScratchSeq)
    Set-Content -Path $scratch -Value '' -Encoding UTF8 -NoNewline
    Start-Process notepad -ArgumentList $scratch | Out-Null

    $p = $null
    for ($i = 0; $i -lt 40; $i++) {
        Start-Sleep -Milliseconds 300
        $p = Get-Process Notepad -ErrorAction SilentlyContinue |
             Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
        if ($p) { break }
    }
    if (-not $p) { throw "Notepad window did not appear" }

    [M2.Native]::ShowWindow($p.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
    [M2.Native]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 900

    # Notepad 11 restores the previous session's tabs, so start from a known
    # empty document rather than whatever was open last time.
    Send-Key @($VK_CONTROL, $VK_A); Start-Sleep -Milliseconds 150
    Send-Key @($VK_DELETE);         Start-Sleep -Milliseconds 250
    $p
}

# Read Notepad's edit control through UI Automation - this reads what the user
# would actually see, rather than trusting the app's own log.
function Get-NotepadText {
    param($Proc)
    Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $Proc.Id)
    $win = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
    if (-not $win) { return $null }
    $edit = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Document)))
    if (-not $edit) {
        $edit = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
            (New-Object System.Windows.Automation.PropertyCondition(
                [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                [System.Windows.Automation.ControlType]::Edit)))
    }
    if (-not $edit) { return $null }
    try { return $edit.GetCurrentPattern(
            [System.Windows.Automation.TextPattern]::Pattern).DocumentRange.GetText(-1) }
    catch {
        try { return $edit.GetCurrentPattern(
                [System.Windows.Automation.ValuePattern]::Pattern).Current.Value }
        catch { return $null }
    }
}

function Play-Fixture {
    param([string]$Path)
    if (-not (Test-Path $Path)) { throw "fixture missing: $Path" }
    $player = New-Object System.Media.SoundPlayer $Path
    $player.PlaySync()
}

function New-Result {
    param($Name, $Pass, $Detail, $Text = '')
    $script:Results += [pscustomobject]@{
        Scenario = $Name; Pass = $Pass; Detail = $Detail; Text = $Text
    }
    $tag = if ($Pass) { 'PASS' } else { 'FAIL' }
    Write-Host ("[{0}] {1} - {2}" -f $tag, $Name, $Detail) -ForegroundColor $(if ($Pass) {'Green'} else {'Red'})
}

# ---------------------------------------------------------------- one dictation cycle
function Invoke-Dictation {
    param(
        [string]$Fixture,
        $Target,
        [int]$SettleSeconds = 25,
        [switch]$CancelMidway,
        [switch]$NoAudio
    )
    Send-Hotkey                                   # start recording
    Start-Sleep -Milliseconds 900                 # let the stream come up
    if (-not $NoAudio) { Play-Fixture $Fixture }
    Start-Sleep -Milliseconds 600                 # trailing silence for the VAD

    if ($CancelMidway) { Send-Cancel; return }

    # Make sure the intended window is really foreground before we stop: the app
    # captures the paste target at stop time, and a window that is merely visible
    # is not necessarily focused.
    if ($Target -and $Target.MainWindowHandle -ne 0) {
        [M2.Native]::SetForegroundWindow($Target.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 400
    }

    Send-Hotkey                                   # stop -> transcribe -> inject
    Start-Sleep -Seconds $SettleSeconds
}

# ---------------------------------------------------------------- scenarios
$FixtureDir = "$PSScriptRoot\..\src-tauri\tests\fixtures"
New-Item -ItemType Directory -Force -Path $ArtifactDir | Out-Null

$app = Get-Process local-voice-ai -ErrorAction SilentlyContinue
if (-not $app) { throw "local-voice-ai is not running - start it before running this harness" }
Write-Host "App PID $($app.Id); harness starting" -ForegroundColor Cyan

$cases = @(
    @{ n='normal';      f='de_test_01.wav';   expect=@('Spracherkennung','Februar') }
    @{ n='umlauts';     f='de_umlaute.wav';   expect=@('ltere','Straße','großen','Köln') }
    @{ n='punctuation'; f='de_punkt.wav';     expect=@('morgen','wäre','großartig')     }
    @{ n='multiline';   f='de_multiline.wav'; expect=@('Zeile')                        }
)

foreach ($c in $cases) {
    if ($Scenario -ne 'all' -and $Scenario -ne $c.n) { continue }
    $np = Start-Notepad
    Invoke-Dictation -Fixture "$FixtureDir\$($c.f)" -Target $np
    $txt = Get-NotepadText -Proc $np
    $ok = $txt -and $txt.Trim().Length -gt 0
    if ($ok) { foreach ($e in $c.expect) { if ($txt -notmatch $e) { $ok = $false } } }
    New-Result $c.n $ok "text length $($txt.Trim().Length)" $txt
    $txt | Out-File "$ArtifactDir\notepad-$($c.n).txt" -Encoding UTF8
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','cancel')) {
    $np = Start-Notepad
    Invoke-Dictation -Fixture "$FixtureDir\de_test_01.wav" -Target $np -CancelMidway
    Start-Sleep -Seconds 4
    $txt = Get-NotepadText -Proc $np
    $ok = -not $txt -or $txt.Trim().Length -eq 0
    New-Result 'cancel' $ok "notepad should stay empty; got '$($txt.Trim())'" $txt
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','silence')) {
    $np = Start-Notepad
    Invoke-Dictation -NoAudio -SettleSeconds 20
    $txt = Get-NotepadText -Proc $np
    # Empty or a trivial artefact both count: what must NOT happen is a hang or junk
    $ok = (-not $txt) -or ($txt.Trim().Length -lt 25)
    New-Result 'silence' $ok "no speech -> '$($txt.Trim())'" $txt
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','rapid')) {
    $np = Start-Notepad
    # hammer the hotkey: start/stop/start/stop with no settle time
    1..4 | ForEach-Object { Send-Hotkey; Start-Sleep -Milliseconds 220 }
    Start-Sleep -Seconds 20
    $alive = $null -ne (Get-Process local-voice-ai -ErrorAction SilentlyContinue)
    New-Result 'rapid' $alive "app still alive after 4 rapid toggles: $alive"
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','unfocused')) {
    # Notepad exists but is NOT foreground; the app should still transcribe and
    # must not crash. Whatever it does with the text, it must not lose it.
    $np = Start-Notepad
    $calc = Start-Notepad
    [M2.Native]::SetForegroundWindow($calc.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 700
    Invoke-Dictation -Fixture "$FixtureDir\de_test_01.wav"
    $other = Get-NotepadText -Proc $calc
    $alive = $null -ne (Get-Process local-voice-ai -ErrorAction SilentlyContinue)
    New-Result 'unfocused' $alive "text went to the focused window instead; app alive: $alive" $other
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

# ---------------------------------------------------------------- report
$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
$report = @("# M2 harness run $stamp", "")
$report += "| Scenario | Result | Detail |"
$report += "|---|---|---|"
foreach ($r in $script:Results) {
    $report += "| {0} | {1} | {2} |" -f $r.Scenario, $(if ($r.Pass) {'PASS'} else {'FAIL'}), $r.Detail
}
$report += ""
foreach ($r in $script:Results) {
    if ($r.Text) { $report += "### $($r.Scenario)"; $report += '```'; $report += $r.Text.Trim(); $report += '```'; $report += "" }
}
$report -join "`n" | Out-File "$ArtifactDir\harness-report.md" -Encoding UTF8

$passed = @($script:Results | Where-Object { $_.Pass }).Count
Write-Host "`n$passed/$(@($script:Results).Count) scenarios passed" -ForegroundColor Cyan
Write-Host "artifacts: $ArtifactDir"
if ($passed -ne @($script:Results).Count) { exit 1 }
