<#
.SYNOPSIS
    Removes the installed research-first-rebuilder personal skill.

.DESCRIPTION
    Deletes ~/.claude/skills/research-first-rebuilder. The versioned source tree under
    tooling/ is never touched, so reinstalling is just running install.ps1 again.

.PARAMETER Force
    Remove without prompting.

.EXAMPLE
    .\uninstall.ps1
#>
[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$SkillName = 'research-first-rebuilder'
$TargetDir = Join-Path $HOME ".claude\skills\$SkillName"

if (-not (Test-Path $TargetDir)) {
    Write-Host "Skill '$SkillName' is not installed at $TargetDir - nothing to do." -ForegroundColor Yellow
    exit 0
}

Write-Host "This will delete: $TargetDir" -ForegroundColor Cyan

if (-not $Force) {
    $answer = Read-Host "Proceed? [y/N]"
    if ($answer -notmatch '^[yY]') {
        Write-Host "Aborted - nothing changed." -ForegroundColor Yellow
        exit 1
    }
}

Remove-Item -Recurse -Force $TargetDir
Write-Host "Removed $SkillName." -ForegroundColor Green
Write-Host "The source tree under tooling/ is untouched; run install.ps1 to reinstall." -ForegroundColor DarkGray
