# Build cue-whisper for Windows
# Requires MSVC or MinGW toolchain
$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $scriptDir

if (Get-Command cl.exe -ErrorAction SilentlyContinue) {
    cl.exe /O2 /Fe:cue-whisper.exe main.c
} elseif (Get-Command gcc.exe -ErrorAction SilentlyContinue) {
    gcc -O2 -o cue-whisper.exe main.c -lm
} else {
    Write-Error "No C compiler found. Install MSVC or MinGW."
    exit 1
}

Write-Host "Built: cue-whisper.exe"
Pop-Location
