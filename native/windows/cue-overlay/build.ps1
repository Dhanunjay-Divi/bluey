$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
New-Item -ItemType Directory -Force -Path build | Out-Null
cl.exe /nologo /O2 /D_WIN32_WINNT=0x0601 /Fe:build\bluey-overlay.exe main.c user32.lib gdi32.lib d2d1.lib dwrite.lib uuid.lib
Copy-Item build\bluey-overlay.exe build\cue-overlay.exe -Force
Write-Output "build\bluey-overlay.exe"
