$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
New-Item -ItemType Directory -Force -Path build | Out-Null
cl.exe /nologo /O2 /D_WIN32_WINNT=0x0601 /Fe:build\bluey-audio.exe main.c ole32.lib uuid.lib
Copy-Item build\bluey-audio.exe build\cue-audio.exe -Force
Write-Output "build\bluey-audio.exe"
