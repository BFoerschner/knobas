# Fail unless both Windows installers `tauri build` made carry the pinned
# PostgreSQL archive: the Windows half of scripts/check-bundled-postgres.sh,
# which says why (docs/decisions/2026-11-14-knobas-ships-its-postgresql.md).
#
#   .msi        unpacked with `msiexec /a` (an administrative install: it
#               copies the files out and installs nothing), the file hashed
#   -setup.exe  NSIS; 7-Zip reads its file table and extracts the one file,
#               which is hashed. Without a 7-Zip that can read it, the
#               installer must at least be larger than the archive, which an
#               installer without it is not (29 MB against 54 MB, 2026-11-14).
#
# Usage: pwsh scripts/check-bundled-postgres.ps1 [-Bundles <bundle-dir>]
param([string]$Bundles = "")

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Bundles) { $Bundles = Join-Path $root 'target/release/bundle' }
$pins = Join-Path $root 'crates/knobas-app/postgresql/archives.sha256'
$target = 'x86_64-pc-windows-msvc'

# `<sha256>  postgresql-<version>-<target>.tar.gz`
$lines = @(Get-Content $pins | Where-Object { $_ -match "^[0-9a-f]{64}  postgresql-.+-$target\.tar\.gz$" })
if ($lines.Count -ne 1) { throw "$pins pins no one archive for $target" }
$want, $asset = $lines[0] -split '  ', 2
$version = $asset.Substring('postgresql-'.Length, $asset.Length - 'postgresql-'.Length - "-$target.tar.gz".Length)
$name = "postgresql-$version.tar.gz"

$checked = 0
$failures = @()
function Verdict([string]$bundle, [string]$got) {
    $script:checked++
    if (-not $got) {
        $script:failures += "$bundle has no postgresql/$name in it"
    } elseif ($got -ne $want) {
        $script:failures += "$bundle carries a postgresql/$name with sha256 $got, and $asset is pinned at $want"
    } else {
        Write-Host "ok   ${bundle}: carries $asset"
    }
}
function Sha256Of($file) {
    if (-not $file) { return '' }
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLower()
}
function Scratch([string]$kind) {
    $dir = Join-Path ([IO.Path]::GetTempPath()) ("knobas-$kind-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $dir | Out-Null
    return $dir
}

foreach ($msi in @(Get-ChildItem -Path (Join-Path $Bundles 'msi') -Filter *.msi -ErrorAction SilentlyContinue)) {
    $out = Scratch 'msi'
    $run = Start-Process -FilePath msiexec.exe -Wait -PassThru -ArgumentList @(
        '/a', "`"$($msi.FullName)`"", '/qn', "TARGETDIR=`"$out`""
    )
    if ($run.ExitCode -ne 0) { throw "msiexec /a exited $($run.ExitCode) on $($msi.Name)" }
    $found = @(Get-ChildItem -Path $out -Recurse -File -Filter $name)
    Verdict $msi.Name (Sha256Of ($found | Select-Object -First 1))
    Remove-Item -Recurse -Force $out
}

$sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
foreach ($exe in @(Get-ChildItem -Path (Join-Path $Bundles 'nsis') -Filter *.exe -ErrorAction SilentlyContinue)) {
    $out = Scratch 'nsis'
    $read = $false
    if ($sevenZip) {
        # `e`: without the installer's folders; `-r`: wherever the file is in it.
        & $sevenZip.Source e "-o$out" -y -r $exe.FullName $name | Out-Null
        $read = ($LASTEXITCODE -eq 0)
    }
    if ($read) {
        $found = @(Get-ChildItem -Path $out -Recurse -File -Filter $name)
        Verdict $exe.Name (Sha256Of ($found | Select-Object -First 1))
    } else {
        # No 7-Zip, or one that cannot read this installer: that is the
        # tool's trouble, not a missing archive. The size still tells.
        $archive = Join-Path $root "crates/knobas-app/postgresql/$name"
        $floor = if (Test-Path $archive) { (Get-Item $archive).Length } else { 50MB }
        Write-Host "::warning::7-Zip did not read $($exe.Name); checking its size against the archive's $floor bytes instead"
        $checked++
        if ($exe.Length -le $floor) {
            $failures += "$($exe.Name) is $($exe.Length) bytes, smaller than the $floor-byte archive it should carry"
        } else {
            Write-Host "ok   $($exe.Name): $($exe.Length) bytes, larger than the archive (not opened)"
        }
    }
    Remove-Item -Recurse -Force $out
}

if ($checked -eq 0) { throw "no installer under $Bundles to check" }
if ($failures.Count -ne 0) {
    $failures | ForEach-Object { Write-Host "FAIL $_" }
    throw "$($failures.Count) of $checked installers lack the pinned PostgreSQL. Run scripts/fetch-postgres.sh before tauri build."
}
Write-Host "check-bundled-postgres: $checked installers carry $asset"
