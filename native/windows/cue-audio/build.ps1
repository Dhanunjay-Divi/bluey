$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path $vswhere)) {
    throw "cl.exe was not found and vswhere.exe is missing. Install Visual Studio Build Tools with the C++ workload."
  }

  $install = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  if (-not $install) {
    throw "cl.exe was not found and no Visual Studio C++ Build Tools installation was found."
  }

  $devcmd = Join-Path $install "Common7\Tools\VsDevCmd.bat"
  if (-not (Test-Path $devcmd)) {
    throw "Visual Studio developer command prompt was not found at $devcmd."
  }

  cmd.exe /c "`"$devcmd`" -arch=x64 -host_arch=x64 >nul && powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`""
  exit $LASTEXITCODE
}

New-Item -ItemType Directory -Force -Path build | Out-Null
cl.exe /nologo /O2 /D_WIN32_WINNT=0x0601 /Fe:build\bluey-audio.exe main.c ole32.lib uuid.lib
Copy-Item build\bluey-audio.exe build\cue-audio.exe -Force
Write-Output "build\bluey-audio.exe"
