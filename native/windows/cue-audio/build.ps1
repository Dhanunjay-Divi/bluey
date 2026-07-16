$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Name,
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [Parameter(Mandatory = $true)]
    [string[]]$Arguments
  )

  & $Executable @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$Name failed with exit code $LASTEXITCODE."
  }
}

function Invoke-TestBinary {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path
  )

  & $Path
  if ($LASTEXITCODE -ne 0) {
    throw "$Path failed with exit code $LASTEXITCODE."
  }
}

function Enter-VisualStudioEnvironment {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
    return $false
  }

  $install = & $vswhere -latest -products * `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath
  if (-not $install) {
    return $false
  }

  $devcmd = Join-Path $install "Common7\Tools\VsDevCmd.bat"
  if (-not (Test-Path -LiteralPath $devcmd -PathType Leaf)) {
    return $false
  }

  cmd.exe /c "`"$devcmd`" -arch=x64 -host_arch=x64 >nul && powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$PSCommandPath`""
  exit $LASTEXITCODE
}

$Msvc = Get-Command cl.exe -ErrorAction SilentlyContinue
$Mingw = Get-Command gcc.exe -ErrorAction SilentlyContinue
if (-not $Msvc -and -not $Mingw) {
  if (-not (Enter-VisualStudioEnvironment)) {
    throw "No supported C compiler found. Install Visual Studio C++ Build Tools or MinGW-w64."
  }
}

New-Item -ItemType Directory -Force -Path build | Out-Null

if ($Msvc) {
  $Common = @(
    "/nologo",
    "/O2",
    "/W4",
    "/WX",
    "/TC",
    '/Fo:build\',
    "/D_WIN32_WINNT=0x0A00"
  )
  Invoke-Checked "MSVC audio argument tests" "cl.exe" ($Common + @(
    "/Fe:build\audio-args-test.exe",
    "audio_args.c",
    "audio_args_test.c"
  ))
  Invoke-Checked "MSVC resampler tests" "cl.exe" ($Common + @(
    "/Fe:build\resampler-test.exe",
    "resampler.c",
    "resampler_test.c"
  ))
  Invoke-TestBinary ".\build\audio-args-test.exe"
  Invoke-TestBinary ".\build\resampler-test.exe"
  Invoke-Checked "MSVC bluey-audio" "cl.exe" ($Common + @(
    "/Fe:build\bluey-audio.exe",
    "main.c",
    "audio_args.c",
    "resampler.c",
    "ole32.lib",
    "uuid.lib",
    "/link",
    "/SUBSYSTEM:CONSOLE,10.00"
  ))
} else {
  $Common = @(
    "-std=c11",
    "-O2",
    "-Wall",
    "-Wextra",
    "-Wpedantic",
    "-Werror",
    "-Wformat=2",
    "-Wstrict-prototypes",
    "-D_WIN32_WINNT=0x0A00"
  )
  Invoke-Checked "MinGW audio argument tests" $Mingw.Source ($Common + @(
    "audio_args.c",
    "audio_args_test.c",
    "-o",
    "build\audio-args-test.exe"
  ))
  Invoke-Checked "MinGW resampler tests" $Mingw.Source ($Common + @(
    "resampler.c",
    "resampler_test.c",
    "-lm",
    "-o",
    "build\resampler-test.exe"
  ))
  Invoke-TestBinary ".\build\audio-args-test.exe"
  Invoke-TestBinary ".\build\resampler-test.exe"
  Invoke-Checked "MinGW bluey-audio" $Mingw.Source ($Common + @(
    "main.c",
    "audio_args.c",
    "resampler.c",
    "-lole32",
    "-luuid",
    "-lm",
    "-Wl,--subsystem,console:10.0",
    "-o",
    "build\bluey-audio.exe"
  ))
}

foreach ($Alias in @("cue-audio.exe", "adriverb.exe", "audio-driver.exe")) {
  Copy-Item "build\bluey-audio.exe" (Join-Path "build" $Alias) -Force
}

Write-Output "build\bluey-audio.exe"
