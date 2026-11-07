<#
.SYNOPSIS
  Builds the bundled local speech worker (sayvane-speech) for Windows x64 with
  MSVC and places it where Tauri expects sidecars:
  src-tauri\binaries\sayvane-speech-x86_64-pc-windows-msvc.exe

.PARAMETER Target
  Target triple. Only x86_64-pc-windows-msvc is supported.

.PARAMETER CpuLevel
  x86-64 instruction level compiled into the engine:
    sse42  (default) conservative, runs on practically every Windows 10 x64 PC
    avx2   Haswell (2013+) and newer; faster, refuses to run on older CPUs
    native build-machine tuning, never ship

.EXAMPLE
  pwsh scripts/build-worker.ps1
  pwsh scripts/build-worker.ps1 -CpuLevel avx2

  Run from a "x64 Native Tools" / Developer PowerShell, or any shell where
  CMake can find Visual Studio 2022 (Build Tools are enough).
  Environment: CMAKE (path to cmake.exe), SAYVANE_ENGINE_TARBALL (optional
  local copy of the pinned engine tarball; the SHA-256 is still verified).
#>
[CmdletBinding()]
param(
    [string]$Target = "x86_64-pc-windows-msvc",
    [ValidateSet("sse42", "avx2", "native")]
    [string]$CpuLevel = "sse42"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($Target -ne "x86_64-pc-windows-msvc") {
    throw "Unsupported target '$Target' (expected x86_64-pc-windows-msvc)."
}

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$Src = Join-Path $Root "speech-worker"
$BuildDir = Join-Path $Src "build\$Target"
$OutDir = Join-Path $Root "src-tauri\binaries"
$Out = Join-Path $OutDir "sayvane-speech-$Target.exe"

# Locate cmake: $env:CMAKE, then PATH, then %USERPROFILE%\.local\cmake-bin\cmake.exe
$Cmake = $null
if ($env:CMAKE) {
    $Cmake = $env:CMAKE
} elseif (Get-Command cmake -ErrorAction SilentlyContinue) {
    $Cmake = (Get-Command cmake).Source
} elseif (Test-Path (Join-Path $HOME ".local\cmake-bin\cmake.exe")) {
    $Cmake = Join-Path $HOME ".local\cmake-bin\cmake.exe"
} else {
    throw "cmake not found (set CMAKE or add cmake to PATH)."
}
Write-Host "Using cmake: $Cmake"

$cfg = @(
    "-S", $Src, "-B", $BuildDir,
    "-A", "x64",
    "-DSAYVANE_X86_LEVEL=$CpuLevel"
)
if ($env:SAYVANE_ENGINE_TARBALL) {
    $cfg += "-DSAYVANE_ENGINE_TARBALL=$($env:SAYVANE_ENGINE_TARBALL)"
}

& $Cmake @cfg
if ($LASTEXITCODE -ne 0) { throw "CMake configure failed." }
& $Cmake --build $BuildDir --config Release --target sayvane-speech --parallel
if ($LASTEXITCODE -ne 0) { throw "CMake build failed." }

$Built = Join-Path $BuildDir "Release\sayvane-speech.exe"
if (-not (Test-Path $Built)) { $Built = Join-Path $BuildDir "sayvane-speech.exe" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Copy-Item -Force $Built $Out

# Verify architecture: PE machine type must be AMD64 (0x8664).
$bytes = [System.IO.File]::ReadAllBytes($Out)
$peOffset = [BitConverter]::ToInt32($bytes, 0x3C)
$machine = [BitConverter]::ToUInt16($bytes, $peOffset + 4)
if ($machine -ne 0x8664) {
    throw ("Unexpected PE machine type 0x{0:X4} (expected 0x8664 / x64)." -f $machine)
}
Write-Host "Built: $Out"
Write-Host ("  PE machine: 0x{0:X4} (x64)" -f $machine)

# Verify dynamic dependencies: only OS DLLs (no VC++ runtime, no OpenMP, no ggml DLLs).
$dumpbin = Get-Command dumpbin -ErrorAction SilentlyContinue
if ($dumpbin) {
    $deps = & dumpbin /nologo /dependents $Out | Where-Object { $_ -match '^\s+\S+\.dll\s*$' } | ForEach-Object { $_.Trim() }
    Write-Host "  dependents: $($deps -join ', ')"
    $bad = $deps | Where-Object { $_ -match '^(vcruntime|msvcp|vcomp|ucrtbased|ggml|whisper|libomp)' }
    if ($bad) { throw "Worker depends on non-system DLLs: $($bad -join ', ')" }
} else {
    Write-Warning "dumpbin not found; run from a Developer PowerShell to verify DLL dependencies."
}
