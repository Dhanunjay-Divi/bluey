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
cl.exe /nologo /O2 /W4 /WX /std:c11 /Fe:build\overlay-protocol-tests.exe /Fo:build\overlay-protocol-tests.obj /TC tests\overlay_protocol_tests.c
if ($LASTEXITCODE -ne 0) {
  throw "Windows overlay protocol tests failed to compile."
}
& .\build\overlay-protocol-tests.exe
if ($LASTEXITCODE -ne 0) {
  throw "Windows overlay protocol tests failed."
}

cl.exe /nologo /O2 /D_WIN32_WINNT=0x0601 /Fe:build\bluey-overlay.exe /Tp main.c user32.lib gdi32.lib d2d1.lib dwrite.lib uuid.lib shell32.lib comctl32.lib advapi32.lib ole32.lib
if ($LASTEXITCODE -ne 0) {
  throw "Windows overlay failed to compile."
}
Copy-Item build\bluey-overlay.exe build\cue-overlay.exe -Force
Copy-Item build\bluey-overlay.exe build\hostovb.exe -Force
Copy-Item build\bluey-overlay.exe build\host-overlay.exe -Force
Write-Output "build\bluey-overlay.exe"
