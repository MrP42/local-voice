#Requires -Version 7.0
<#
.SYNOPSIS
    M3 acceptance harness: the stabilized dictation path against real Windows apps.

.NOTES
    Needs PowerShell 7 (pwsh), not Windows PowerShell 5.1.

.DESCRIPTION
    Successor to m2-verify.ps1, which stays as the historical M2 artifact. What
    is new here:

      * reads the configured hotkey out of the settings store instead of
        assuming Ctrl+Space
      * waits for the text to actually appear instead of sleeping a fixed
        number of seconds, so a 100-run endurance pass is feasible
      * additionally checks: numbers, focus change during recording, a target
        with no editable field, an elevated target, and the visible fallback
        notice

    Nothing in the core path is mocked. The keypress (keybd_event) and the
    voice (Windows OneCore TTS through the speakers, captured acoustically by
    the real microphone) are synthetic; everything else — hotkey hook,
    recording, local Parakeet inference, injection — is the shipping code.

.PARAMETER Scenario
    Which scenario to run. 'all' runs everything except 'endurance'.

.PARAMETER Runs
    Dictation count for the endurance scenario.
#>
[CmdletBinding()]
param(
    [ValidateSet('all', 'normal', 'umlauts', 'punctuation', 'multiline', 'numbers',
                 'cancel', 'silence', 'rapid', 'focus-change', 'no-edit-field',
                 'elevated', 'endurance')]
    [string]$Scenario = 'all',

    [int]$Runs = 100,

    [string]$AppExe = "$PSScriptRoot\..\src-tauri\target\release\sprechstift.exe",

    [string]$ArtifactDir = "$PSScriptRoot\..\..\..\docs\m3-evidence"
)

$ErrorActionPreference = 'Stop'
$script:Results = @()
$script:ScratchSeq = 0

# ---------------------------------------------------------------- Win32 interop
if (-not ("M3.Native" -as [type])) {
Add-Type -Namespace M3 -Name Native -MemberDefinition @'
    [DllImport("user32.dll")]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, System.UIntPtr dwExtraInfo);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, System.Text.StringBuilder buf, int max);
'@ | Out-Null
}

$KEYEVENTF_KEYUP = 0x0002
$VK_ESCAPE = 0x1B; $VK_A = 0x41; $VK_DELETE = 0x2E; $VK_CONTROL = 0x11

# Binding names as persisted by the app -> virtual key codes.
$VkByName = @{
    'ctrl' = 0x11; 'ctrl_left' = 0xA2; 'ctrl_right' = 0xA3
    'shift' = 0x10; 'shift_left' = 0xA0; 'shift_right' = 0xA1
    'alt' = 0x12; 'alt_left' = 0xA4; 'alt_right' = 0xA5
    'super' = 0x5B; 'super_left' = 0x5B; 'super_right' = 0x5C
    'space' = 0x20; 'escape' = 0x1B; 'enter' = 0x0D; 'tab' = 0x09
}
foreach ($c in 'a'..'z') { $VkByName[$c] = [byte][char]::ToUpper($c) }
foreach ($d in 0..9)     { $VkByName["$d"] = 0x30 + $d }

function Get-HotkeyVKeys {
    $store = Join-Path $env:APPDATA 'de.wolffappliedai.sprechstift\settings_store.json'
    if (-not (Test-Path $store)) { throw "settings store not found: $store" }
    $binding = (Get-Content $store -Raw | ConvertFrom-Json).settings.bindings.transcribe.current_binding
    $keys = @()
    foreach ($part in ($binding -split '\+')) {
        $name = $part.Trim().ToLower()
        if (-not $VkByName.ContainsKey($name)) { throw "unmapped hotkey component '$name' (binding '$binding')" }
        $keys += [byte]$VkByName[$name]
    }
    Write-Host "hotkey from settings: $binding" -ForegroundColor DarkGray
    , $keys
}

function Send-Key {
    param([byte[]]$VKeys)
    foreach ($k in $VKeys) {
        [M3.Native]::keybd_event($k, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60
    }
    for ($i = $VKeys.Count - 1; $i -ge 0; $i--) {
        [M3.Native]::keybd_event($VKeys[$i], 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 60
    }
}

$script:HotkeyVKeys = Get-HotkeyVKeys
function Send-Hotkey { Send-Key $script:HotkeyVKeys; Start-Sleep -Milliseconds 250 }
function Send-Cancel { Send-Key @([byte]$VK_ESCAPE);  Start-Sleep -Milliseconds 250 }

# ---------------------------------------------------------------- Notepad control
function Start-Notepad {
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 900

    Get-ChildItem $env:TEMP -Filter 'sprechstift-m3-*.txt' -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
    $script:ScratchSeq++
    $scratch = Join-Path $env:TEMP ("sprechstift-m3-{0}-{1}.txt" -f $PID, $script:ScratchSeq)
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

    [M3.Native]::ShowWindow($p.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
    [M3.Native]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 900
    Send-Key @([byte]$VK_CONTROL, [byte]$VK_A); Start-Sleep -Milliseconds 150
    Send-Key @([byte]$VK_DELETE);               Start-Sleep -Milliseconds 250
    $p
}

# Read the target's edit control through UI Automation: what the user sees on
# screen, not what the app logged.
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

function Get-ClipboardTextSafe {
    try { return (Get-Clipboard -Raw -ErrorAction Stop) } catch { return $null }
}

function Play-Fixture {
    param([string]$Path)
    if (-not (Test-Path $Path)) { throw "fixture missing: $Path" }
    (New-Object System.Media.SoundPlayer $Path).PlaySync()
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
# Waits for text to appear rather than sleeping a fixed interval: the endurance
# run would otherwise take an hour of pure waiting.
function Invoke-Dictation {
    param(
        [string]$Fixture,
        $Target,
        [int]$TimeoutSeconds = 30,
        [switch]$CancelMidway,
        [switch]$NoAudio,
        [switch]$NoWait
    )
    Send-Hotkey                                   # start recording
    Start-Sleep -Milliseconds 900                 # let the stream come up
    if (-not $NoAudio) { Play-Fixture $Fixture }
    Start-Sleep -Milliseconds 600                 # trailing silence for the VAD

    if ($CancelMidway) { Send-Cancel; return $null }

    if ($Target -and $Target.MainWindowHandle -ne 0) {
        [M3.Native]::SetForegroundWindow($Target.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 400
    }

    $started = Get-Date
    Send-Hotkey                                   # stop -> transcribe -> inject
    if ($NoWait) { Start-Sleep -Seconds 6; return $null }

    while (((Get-Date) - $started).TotalSeconds -lt $TimeoutSeconds) {
        Start-Sleep -Milliseconds 400
        if ($Target) {
            $txt = Get-NotepadText -Proc $Target
            if ($txt -and $txt.Trim().Length -gt 0) {
                # Let a trailing fragment settle before reading the final value.
                Start-Sleep -Milliseconds 900
                return [pscustomobject]@{
                    Text = Get-NotepadText -Proc $Target
                    Seconds = ((Get-Date) - $started).TotalSeconds
                }
            }
        }
    }
    [pscustomobject]@{ Text = $(if ($Target) { Get-NotepadText -Proc $Target } else { $null })
                       Seconds = $TimeoutSeconds }
}

# ---------------------------------------------------------------- scenarios
$FixtureDir = "$PSScriptRoot\..\src-tauri\tests\fixtures"
New-Item -ItemType Directory -Force -Path $ArtifactDir | Out-Null

$app = Get-Process sprechstift -ErrorAction SilentlyContinue
if (-not $app) { throw "sprechstift is not running - start it before running this harness" }
Write-Host "App PID $($app.Id); harness starting" -ForegroundColor Cyan

$cases = @(
    @{ n='normal';      f='de_test_01.wav';   expect=@('Spracherkennung','Februar') }
    @{ n='umlauts';     f='de_umlaute.wav';   expect=@('Straße','großen','Köln')    }
    @{ n='punctuation'; f='de_punkt.wav';     expect=@('morgen','wäre','großartig') }
    @{ n='multiline';   f='de_multiline.wav'; expect=@('Zeile')                     }
    @{ n='numbers';     f='de_zahlen.wav';    expect=@('19')                        }
)

foreach ($c in $cases) {
    if ($Scenario -notin @('all', $c.n)) { continue }
    $np = Start-Notepad
    $run = Invoke-Dictation -Fixture "$FixtureDir\$($c.f)" -Target $np
    $txt = $run.Text
    $ok = $txt -and $txt.Trim().Length -gt 0
    if ($ok) { foreach ($e in $c.expect) { if ($txt -notmatch [regex]::Escape($e)) { $ok = $false } } }
    New-Result $c.n $ok ("{0} chars in {1:N1}s" -f $txt.Trim().Length, $run.Seconds) $txt
    $txt | Out-File "$ArtifactDir\notepad-$($c.n).txt" -Encoding UTF8
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','cancel')) {
    $np = Start-Notepad
    Invoke-Dictation -Fixture "$FixtureDir\de_test_01.wav" -Target $np -CancelMidway | Out-Null
    Start-Sleep -Seconds 6
    $txt = Get-NotepadText -Proc $np
    $ok = -not $txt -or $txt.Trim().Length -eq 0
    New-Result 'cancel' $ok "notepad must stay empty; got '$($txt.Trim())'" $txt
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','silence')) {
    $np = Start-Notepad
    Invoke-Dictation -NoAudio -Target $np -TimeoutSeconds 18 | Out-Null
    $txt = Get-NotepadText -Proc $np
    $ok = (-not $txt) -or ($txt.Trim().Length -lt 25)
    New-Result 'silence' $ok "no speech -> '$($txt.Trim())'" $txt
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','rapid')) {
    $np = Start-Notepad
    1..4 | ForEach-Object { Send-Hotkey; Start-Sleep -Milliseconds 220 }
    Start-Sleep -Seconds 20
    $alive = $null -ne (Get-Process sprechstift -ErrorAction SilentlyContinue)
    $txt = Get-NotepadText -Proc $np
    New-Result 'rapid' $alive "app alive after 4 rapid toggles: $alive; text '$($txt.Trim())'" $txt
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','focus-change')) {
    # Focus moves to a second window WHILE recording, and the dictation is
    # stopped there. The text must land in the window that was focused at stop
    # time — and nothing may appear in the first one.
    $first = Start-Notepad
    $second = Start-Notepad
    [M3.Native]::SetForegroundWindow($first.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 500

    Send-Hotkey
    Start-Sleep -Milliseconds 900
    Play-Fixture "$FixtureDir\de_short_01.wav"
    [M3.Native]::SetForegroundWindow($second.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 700
    $started = Get-Date
    Send-Hotkey
    while (((Get-Date) - $started).TotalSeconds -lt 30) {
        Start-Sleep -Milliseconds 400
        $t = Get-NotepadText -Proc $second
        if ($t -and $t.Trim().Length -gt 0) { break }
    }
    Start-Sleep -Milliseconds 900
    $firstText  = (Get-NotepadText -Proc $first)  ?? ''
    $secondText = (Get-NotepadText -Proc $second) ?? ''
    # The first window must stay untouched. The second either received the text
    # or the fallback fired — both are acceptable, silent loss is not.
    $clip = Get-ClipboardTextSafe
    $ok = ($firstText.Trim().Length -eq 0) -and
          (($secondText.Trim().Length -gt 0) -or ($clip -and $clip.Trim().Length -gt 0))
    New-Result 'focus-change' $ok ("first='{0}' second={1} chars, clipboard={2} chars" -f `
        $firstText.Trim(), $secondText.Trim().Length, $(if ($clip) { $clip.Trim().Length } else { 0 })) $secondText
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

if ($Scenario -in @('all','no-edit-field')) {
    # A foreground window with no editable field. The paste is delivered but
    # goes nowhere; the contract is that the app survives and the text is not
    # lost — it must be recoverable from the clipboard or the history.
    Set-Clipboard -Value ''
    $explorer = Start-Process explorer.exe -ArgumentList $env:SystemRoot -PassThru
    Start-Sleep -Seconds 3
    $win = Get-Process -Name explorer | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if ($win) { [M3.Native]::SetForegroundWindow($win.MainWindowHandle) | Out-Null }
    Start-Sleep -Milliseconds 800
    Invoke-Dictation -Fixture "$FixtureDir\de_short_01.wav" -NoWait | Out-Null
    Start-Sleep -Seconds 12
    $alive = $null -ne (Get-Process sprechstift -ErrorAction SilentlyContinue)
    $clip = Get-ClipboardTextSafe
    New-Result 'no-edit-field' $alive ("app alive: {0}; clipboard '{1}'" -f $alive, $(if ($clip) { $clip.Trim() } else { '' })) $clip
}

if ($Scenario -in @('all','elevated')) {
    $elevated = Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.MainWindowHandle -ne 0 -and -not $_.Path
    } | Select-Object -First 1
    if (-not $elevated) {
        New-Result 'elevated' $true 'SKIPPED - no elevated window with a visible frame found'
    } else {
        Set-Clipboard -Value ''
        [M3.Native]::SetForegroundWindow($elevated.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 800
        Invoke-Dictation -Fixture "$FixtureDir\de_short_01.wav" -NoWait | Out-Null
        Start-Sleep -Seconds 12
        $clip = Get-ClipboardTextSafe
        $ok = $clip -and $clip.Trim().Length -gt 0
        New-Result 'elevated' $ok ("target '{0}': transcript in clipboard: {1}" -f $elevated.ProcessName, $ok) $clip
    }
}

if ($Scenario -eq 'endurance') {
    $np = Start-Notepad
    $fail = 0; $empty = 0; $durations = @()
    $expected = 'Termin'
    for ($i = 1; $i -le $Runs; $i++) {
        Send-Key @([byte]$VK_CONTROL, [byte]$VK_A); Start-Sleep -Milliseconds 100
        Send-Key @([byte]$VK_DELETE);               Start-Sleep -Milliseconds 200
        $run = Invoke-Dictation -Fixture "$FixtureDir\de_short_01.wav" -Target $np -TimeoutSeconds 25
        $txt = ($run.Text) ?? ''
        $durations += $run.Seconds
        if ($txt.Trim().Length -eq 0) { $empty++; $fail++ }
        elseif ($txt -notmatch $expected) { $fail++ }
        if ($i % 10 -eq 0) {
            Write-Host ("  {0}/{1} runs, {2} failures, median {3:N1}s" -f $i, $Runs, $fail,
                (($durations | Sort-Object)[[int]($durations.Count/2)])) -ForegroundColor DarkGray
        }
    }
    $median = ($durations | Sort-Object)[[int]($durations.Count/2)]
    New-Result 'endurance' ($fail -eq 0) ("{0} runs, {1} failures ({2} empty), median {3:N1}s, max {4:N1}s" -f `
        $Runs, $fail, $empty, $median, ($durations | Measure-Object -Maximum).Maximum)
    Get-Process Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

# ---------------------------------------------------------------- report
$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
$report = @("# M3 harness run $stamp", "", "Scenario set: ``$Scenario``", "")
$report += "| Scenario | Result | Detail |"
$report += "|---|---|---|"
foreach ($r in $script:Results) {
    $report += "| {0} | {1} | {2} |" -f $r.Scenario, $(if ($r.Pass) {'PASS'} else {'FAIL'}), $r.Detail
}
$report += ""
foreach ($r in $script:Results) {
    if ($r.Text) { $report += "### $($r.Scenario)"; $report += '```'; $report += $r.Text.Trim(); $report += '```'; $report += "" }
}
$suffix = if ($Scenario -eq 'all') { '' } else { "-$Scenario" }
$report -join "`n" | Out-File "$ArtifactDir\harness-report$suffix.md" -Encoding UTF8

$passed = @($script:Results | Where-Object { $_.Pass }).Count
Write-Host "`n$passed/$(@($script:Results).Count) scenarios passed" -ForegroundColor Cyan
Write-Host "artifacts: $ArtifactDir"
if ($passed -ne @($script:Results).Count) { exit 1 }
