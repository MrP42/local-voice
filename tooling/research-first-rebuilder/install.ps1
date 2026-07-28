<#
.SYNOPSIS
    Installs the research-first-rebuilder skill as a personal Claude Code skill.

.DESCRIPTION
    Deploys the versioned source in this directory to the personal skill directory
    (~/.claude/skills/research-first-rebuilder). The source tree stays the single
    source of truth; the installed copy is a deployment artifact, so there is never
    a second version to keep in sync.

.PARAMETER Force
    Overwrite an existing installation without prompting.

.EXAMPLE
    .\install.ps1
    .\install.ps1 -Force
#>
[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$SkillName = 'research-first-rebuilder'
$SourceDir = $PSScriptRoot
$TargetDir = Join-Path $HOME ".claude\skills\$SkillName"

# Everything the skill needs at runtime. Dev-only material (evals, workspaces) is
# deliberately excluded so the installed skill stays small and loads fast.
$Include = @('SKILL.md', 'README.md', 'VERSION', 'CHANGELOG.md', 'references', 'assets', 'scripts')

Write-Host "Installing skill '$SkillName'" -ForegroundColor Cyan
Write-Host "  source: $SourceDir"
Write-Host "  target: $TargetDir"

$manifest = Join-Path $SourceDir 'SKILL.md'
if (-not (Test-Path $manifest)) {
    throw "SKILL.md not found in $SourceDir - run this script from the skill source directory."
}

if (Test-Path $TargetDir) {
    if (-not $Force) {
        $answer = Read-Host "Target already exists. Replace it? [y/N]"
        if ($answer -notmatch '^[yY]') {
            Write-Host "Aborted - nothing changed." -ForegroundColor Yellow
            exit 1
        }
    }
    Remove-Item -Recurse -Force $TargetDir
}

New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null

foreach ($item in $Include) {
    $src = Join-Path $SourceDir $item
    if (-not (Test-Path $src)) {
        Write-Host "  skip (absent): $item" -ForegroundColor DarkGray
        continue
    }
    Copy-Item -Path $src -Destination $TargetDir -Recurse -Force
    Write-Host "  copied: $item" -ForegroundColor DarkGray
}

$version = if (Test-Path (Join-Path $SourceDir 'VERSION')) {
    (Get-Content (Join-Path $SourceDir 'VERSION') -Raw).Trim()
} else { 'unknown' }

$fileCount = (Get-ChildItem -Recurse -File $TargetDir | Measure-Object).Count

Write-Host ""
Write-Host "Installed $SkillName v$version ($fileCount files)." -ForegroundColor Green
Write-Host "Invoke with: /$SkillName <target-url> [platform] [constraints] [distribution-mode] [output-dir]"
Write-Host ""
Write-Host "Claude Code watches ~/.claude/skills and picks up changes within the running" -ForegroundColor DarkGray
Write-Host "session. If the command does not appear, start a new session." -ForegroundColor DarkGray
