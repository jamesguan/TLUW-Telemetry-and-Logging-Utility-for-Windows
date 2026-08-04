# Semantic version bump for TLUW releases (used by GitHub Actions).
# Rules (Conventional Commits since last v* tag):
#   BREAKING CHANGE / feat!: / fix!: / …! → major
#   feat:                                 → minor
#   anything else (fix, perf, chore, …) → patch
# Every merge to main that is not itself a release commit gets at least a patch bump
# once a v* tag already exists. First release ships Cargo.toml as-is.

param(
    [ValidateSet("auto", "major", "minor", "patch")]
    [string]$Bump = "auto",

    [switch]$WriteFiles
)

$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path (Join-Path $PSScriptRoot "..\.."))

function Get-CargoVersion {
    $m = Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $m) { throw "Could not read version from Cargo.toml" }
    return $m.Matches[0].Groups[1].Value
}

function Get-LastVersionTag {
    $tag = git describe --tags --abbrev=0 --match "v*" 2>$null
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($tag)) {
        return $null
    }
    return $tag.Trim()
}

function Parse-SemVer([string]$v) {
    $v = $v.TrimStart("v")
    if ($v -notmatch '^(\d+)\.(\d+)\.(\d+)') {
        throw "Invalid semver: $v"
    }
    return [pscustomobject]@{
        Major = [int]$Matches[1]
        Minor = [int]$Matches[2]
        Patch = [int]$Matches[3]
    }
}

function Format-SemVer($sv) {
    return "{0}.{1}.{2}" -f $sv.Major, $sv.Minor, $sv.Patch
}

function Apply-Bump($sv, [string]$kind) {
    switch ($kind) {
        "major" { $sv.Major++; $sv.Minor = 0; $sv.Patch = 0 }
        "minor" { $sv.Minor++; $sv.Patch = 0 }
        "patch" { $sv.Patch++ }
        default { throw "Unknown bump: $kind" }
    }
    return $sv
}

function Get-AutoBump([string]$range) {
    $args = @("log", "--pretty=%s%n%b%n---COMMIT---")
    if ($range) { $args += $range }
    $text = & git @args 2>$null
    if (-not $text) { return "patch" }

    $joined = ($text -join "`n")
    if ($joined -match '(?m)^BREAKING CHANGE:' -or $joined -match '(?m)^\w+(\([^)]*\))?!:') {
        return "major"
    }
    if ($joined -match '(?m)^feat(\([^)]*\))?:') {
        return "minor"
    }
    return "patch"
}

function Set-CargoVersion([string]$newVersion) {
    $current = Get-CargoVersion
    if ($current -eq $newVersion) {
        Write-Host "Cargo.toml already at $newVersion (no file rewrite needed)"
        return
    }

    $toml = Get-Content "Cargo.toml" -Raw
    $toml2 = [regex]::Replace($toml, '(?m)^version\s*=\s*"[^"]+"', "version = `"$newVersion`"")
    if ((Get-CargoVersionFromText $toml2) -ne $newVersion) {
        throw "Failed to update Cargo.toml version (still not $newVersion)"
    }
    Set-Content -Path "Cargo.toml" -Value $toml2 -NoNewline

    $lock = Get-Content "Cargo.lock" -Raw
    # Only the root package block.
    $lock2 = [regex]::Replace(
        $lock,
        '(?s)(name = "telemetry-logging-utility"\r?\n)version = "[^"]+"',
        "`${1}version = `"$newVersion`""
    )
    if ($lock2 -eq $lock) {
        Write-Warning "Cargo.lock root package version not updated (pattern miss or already current)"
    } else {
        Set-Content -Path "Cargo.lock" -Value $lock2 -NoNewline
    }
}

function Get-CargoVersionFromText([string]$text) {
    $m = [regex]::Match($text, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $m.Success) { return $null }
    return $m.Groups[1].Value
}

function Compare-SemVer($a, $b) {
    if ($a.Major -ne $b.Major) { return $a.Major - $b.Major }
    if ($a.Minor -ne $b.Minor) { return $a.Minor - $b.Minor }
    return $a.Patch - $b.Patch
}

$cargoVer = Get-CargoVersion
$lastTag = Get-LastVersionTag
$cargoSv = Parse-SemVer $cargoVer

if ($Bump -eq "auto") {
    if ($lastTag) {
        $tagSv = Parse-SemVer $lastTag
        if ((Compare-SemVer $cargoSv $tagSv) -gt 0) {
            # Cargo.toml was manually raised (e.g. 0.4 → 1.0); ship it as-is.
            $kind = "catch-up"
            $next = $cargoVer
            $reason = "Cargo.toml $cargoVer is ahead of $lastTag - shipping catch-up"
        } else {
            $range = "$lastTag..HEAD"
            $kind = Get-AutoBump $range
            $nextSv = Apply-Bump $tagSv $kind
            $next = Format-SemVer $nextSv
            $reason = "auto/$kind from $lastTag"
        }
    } else {
        $kind = "initial"
        $next = $cargoVer
        $reason = "initial release from Cargo.toml"
    }
} else {
    $kind = $Bump
    if ($lastTag) {
        $tagSv = Parse-SemVer $lastTag
        if ((Compare-SemVer $cargoSv $tagSv) -gt 0) {
            $base = $cargoSv
        } else {
            $base = $tagSv
        }
    } else {
        $base = $cargoSv
    }
    $nextSv = Apply-Bump ([pscustomobject]@{
        Major = $base.Major; Minor = $base.Minor; Patch = $base.Patch
    }) $kind
    $next = Format-SemVer $nextSv
    $reason = "manual/$kind"
}

# No-op guard: if next equals last tag, force patch bump
if ($lastTag -and ("v$next" -eq $lastTag)) {
    $nextSv = Apply-Bump (Parse-SemVer $next) "patch"
    $next = Format-SemVer $nextSv
    $kind = "patch"
    $reason = "$reason (forced patch; tag already existed)"
}

Write-Host "Cargo.toml was: $cargoVer"
Write-Host "Last tag:        $(if ($lastTag) { $lastTag } else { '(none)' })"
Write-Host "Next version:    $next  ($reason)"

if ($WriteFiles) {
    Set-CargoVersion $next
    Write-Host "Wrote Cargo.toml / Cargo.lock -> $next"
}

if ($env:GITHUB_OUTPUT) {
    "version=$next" >> $env:GITHUB_OUTPUT
    "tag=v$next" >> $env:GITHUB_OUTPUT
    "bump=$kind" >> $env:GITHUB_OUTPUT
    "reason=$reason" >> $env:GITHUB_OUTPUT
}
