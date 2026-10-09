<#
.SYNOPSIS
    Stage everything the upscayl module runs into modules/upscayl/engine/.

.DESCRIPTION
    Copies upstream's engine binaries (all three platforms), every model whose license
    is recorded in licenses/models.json, and a pinned, hash-checked exiftool build into
    the module's engine/ folder (gitignored), then writes engine/manifest.json listing
    every staged file with its sha256 and where it came from.

    Engine files are upstream's and are never re-committed (decisions.md). `modules pack`
    only packs what is inside the module directory, which is why they are staged here.

    Idempotent: a file whose hash already matches is left alone, and a second run with
    nothing changed reports no changes.

.PARAMETER NoNonCommercial
    Skip models whose license forbids commercial use (commercial_use = forbidden).

.PARAMETER SkipExiftool
    Do not download or stage exiftool (copy_metadata will then refuse to run).

.EXAMPLE
    pwsh modules/upscayl/scripts/sync-engine.ps1
#>
[CmdletBinding()]
param(
    [switch]$NoNonCommercial,
    [switch]$SkipExiftool
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Pinned exiftool build. exiftool.org links its Windows builds to SourceForge, which
# keeps every version. The direct mirror URL needs a non-browser User-Agent, or
# SourceForge answers with an HTML interstitial (which the hash check would reject).
# Bump the version and hash together, from https://exiftool.org/checksums.txt.
$ExiftoolVersion = '13.59'
$ExiftoolSha256  = '44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec'
$ExiftoolUrls    = @(
    "https://downloads.sourceforge.net/project/exiftool/exiftool-${ExiftoolVersion}_64.zip"
)

$ModuleDir = Split-Path -Parent $PSScriptRoot
$EngineDir = Join-Path $ModuleDir 'engine'

# The fork root is found by its files, never by a directory name (INTEROP.md, rule 8).
function Find-ForkRoot([string]$start) {
    $dir = Get-Item $start
    while ($dir) {
        $pkg = Join-Path $dir.FullName 'package.json'
        if ((Test-Path (Join-Path $dir.FullName 'resources/models')) -and (Test-Path $pkg) -and
            ((Get-Content $pkg -Raw) -match '"name"\s*:\s*"upscayl"')) {
            return $dir.FullName
        }
        $dir = $dir.Parent
    }
    throw "Could not find the upscayl fork root above $start (looked for resources/models and an upscayl package.json)."
}

function Get-Sha256([string]$path) { (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() }

$script:Changed = 0
$script:Unchanged = 0
$script:Entries = [System.Collections.Generic.List[object]]::new()

function Stage-File([string]$source, [string]$relDest, [string]$origin) {
    $dest = Join-Path $EngineDir $relDest
    $hash = Get-Sha256 $source
    if ((Test-Path -LiteralPath $dest) -and ((Get-Sha256 $dest) -eq $hash)) {
        $script:Unchanged++
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dest) | Out-Null
        Copy-Item -LiteralPath $source -Destination $dest -Force
        $script:Changed++
        Write-Host "  staged  $relDest"
    }
    $script:Entries.Add([ordered]@{ path = $relDest.Replace('\', '/'); sha256 = $hash; source = $origin })
}

$Root = Find-ForkRoot $ModuleDir
Write-Host "fork root: $Root"
New-Item -ItemType Directory -Force -Path $EngineDir | Out-Null

# --- Engine binaries, every platform (only Windows is tested; decisions.md) ---------
Write-Host 'engine binaries:'
foreach ($platform in 'win', 'linux', 'mac') {
    $binDir = Join-Path $Root "resources/$platform/bin"
    if (-not (Test-Path $binDir)) { throw "Missing upstream engine folder: $binDir" }
    foreach ($f in Get-ChildItem -LiteralPath $binDir -File) {
        Stage-File $f.FullName "bin/$platform/$($f.Name)" "resources/$platform/bin/$($f.Name)"
    }
}

# --- Models, gated on licenses/models.json ------------------------------------------
Write-Host 'models:'
$licenses = Get-Content -Raw (Join-Path $ModuleDir 'licenses/models.json') | ConvertFrom-Json
$skipped = @()
foreach ($m in $licenses.models) {
    if (-not $m.license -or $m.license -eq 'unknown') {
        $skipped += "$($m.name): license is unknown"; continue
    }
    if ($NoNonCommercial -and $m.commercial_use -eq 'forbidden') {
        $skipped += "$($m.name): non-commercial ($($m.license)) and -NoNonCommercial was given"; continue
    }
    foreach ($ext in 'param', 'bin') {
        $src = Join-Path $Root "$($m.source_dir)/$($m.name).$ext"
        if (-not (Test-Path -LiteralPath $src)) { throw "licenses/models.json names $($m.name) but $src does not exist." }
        Stage-File $src "models/$($m.name).$ext" "$($m.source_dir)/$($m.name).$ext"
    }
}
# A model file upstream ships that has no license entry is reported, never staged.
$known = @($licenses.models | ForEach-Object { $_.name })
foreach ($dir in 'resources/models', 'models') {
    foreach ($f in Get-ChildItem -LiteralPath (Join-Path $Root $dir) -Filter *.param -File) {
        if ($known -notcontains $f.BaseName) { $skipped += "$($f.BaseName): no entry in licenses/models.json" }
    }
}
# Remove staged models that are no longer allowed, so a re-run reflects the current gate.
$allowedFiles = @($script:Entries | Where-Object { $_.path -like 'models/*' } | ForEach-Object { $_.path })
$stagedModels = Join-Path $EngineDir 'models'
if (Test-Path $stagedModels) {
    foreach ($f in Get-ChildItem -LiteralPath $stagedModels -File) {
        if ($allowedFiles -notcontains "models/$($f.Name)") {
            Remove-Item -LiteralPath $f.FullName
            $script:Changed++
            Write-Host "  removed models/$($f.Name)"
        }
    }
}
foreach ($s in $skipped) { Write-Host "  skipped $s" }

# --- exiftool -------------------------------------------------------------------------
$exifRecord = $null
if (-not $SkipExiftool) {
    Write-Host 'exiftool:'
    $exifDir = Join-Path $EngineDir 'exiftool'
    $exe = Join-Path $exifDir 'exiftool.exe'
    $stamp = Join-Path $exifDir 'VERSION'
    if ((Test-Path $exe) -and (Test-Path $stamp) -and ((Get-Content $stamp -Raw).Trim() -eq "$ExiftoolVersion $ExiftoolSha256")) {
        $script:Unchanged++
    } else {
        $tmp = Join-Path ([IO.Path]::GetTempPath()) "upscayl-exiftool-$ExiftoolVersion"
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $tmp | Out-Null
        $zip = Join-Path $tmp 'exiftool.zip'
        $got = $false
        foreach ($url in $ExiftoolUrls) {
            try { Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing -UserAgent 'Wget'; $got = $true; break }
            catch { Write-Host "  download failed from $url : $($_.Exception.Message)" }
        }
        if (-not $got) { throw "Could not download exiftool $ExiftoolVersion from any source." }
        $actual = Get-Sha256 $zip
        if ($actual -ne $ExiftoolSha256) {
            throw "exiftool download hash mismatch: expected $ExiftoolSha256, got $actual. Nothing was staged."
        }
        Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force
        $inner = Get-ChildItem -LiteralPath $tmp -Directory | Where-Object { $_.Name -like 'exiftool-*' } | Select-Object -First 1
        if (-not $inner) { throw 'Unexpected exiftool archive layout: no exiftool-* folder.' }
        $kexe = Get-ChildItem -LiteralPath $inner.FullName -Filter 'exiftool*.exe' | Select-Object -First 1
        if (-not $kexe) { throw 'Unexpected exiftool archive layout: no exiftool executable.' }
        Remove-Item -Recurse -Force $exifDir -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $exifDir | Out-Null
        Copy-Item -LiteralPath $kexe.FullName -Destination $exe
        $files = Join-Path $inner.FullName 'exiftool_files'
        if (Test-Path $files) { Copy-Item -Recurse -LiteralPath $files -Destination (Join-Path $exifDir 'exiftool_files') }
        Set-Content -Path (Join-Path $exifDir 'LICENSE.txt') -Encoding utf8 -Value @(
            "ExifTool $ExiftoolVersion by Phil Harvey, https://exiftool.org/",
            'This is free software; you can redistribute it and/or modify it under the same terms as Perl itself',
            '(the Artistic License or the GNU General Public License, version 1 or later).',
            "Downloaded from $($ExiftoolUrls[0]), sha256 $ExiftoolSha256."
        )
        Set-Content -Path $stamp -Encoding ascii -Value "$ExiftoolVersion $ExiftoolSha256"
        Remove-Item -Recurse -Force $tmp
        $script:Changed++
        Write-Host "  staged  exiftool $ExiftoolVersion"
    }
    $probe = & $exe -ver 2>&1
    if ("$probe".Trim() -ne $ExiftoolVersion) { throw "Staged exiftool reports '$probe', expected $ExiftoolVersion." }
    $exifRecord = [ordered]@{ version = $ExiftoolVersion; sha256_zip = $ExiftoolSha256; path = 'exiftool/exiftool.exe' }
}

# --- Manifest -------------------------------------------------------------------------
$upstreamCommit = (& git -C $Root log -1 --format=%H -- resources models 2>$null)
$manifest = [ordered]@{
    schema           = 1
    upstream_commit  = "$upstreamCommit".Trim()
    no_non_commercial = [bool]$NoNonCommercial
    exiftool         = $exifRecord
    skipped          = $skipped
    files            = @($script:Entries | Sort-Object { $_.path })
}
$json = ($manifest | ConvertTo-Json -Depth 5) -replace "`r`n", "`n"
$manifestPath = Join-Path $EngineDir 'manifest.json'
$old = if (Test-Path $manifestPath) { (Get-Content -Raw $manifestPath) -replace "`r`n", "`n" } else { '' }
if ($old.TrimEnd() -ne $json.TrimEnd()) {
    [IO.File]::WriteAllText($manifestPath, $json + "`n")
    if ($old) { $script:Changed++ }
}

if ($script:Changed -eq 0) {
    Write-Host "no changes ($($script:Unchanged) files already staged)"
} else {
    Write-Host "$($script:Changed) change(s), $($script:Unchanged) unchanged"
}
