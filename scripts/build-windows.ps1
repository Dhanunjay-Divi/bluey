$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$CargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $CargoBin) {
  $env:Path = "$CargoBin;$env:Path"
}

cargo build --release
powershell -ExecutionPolicy Bypass -File native\windows\cue-overlay\build.ps1 | Out-Null
powershell -ExecutionPolicy Bypass -File native\windows\cue-audio\build.ps1 | Out-Null

$Dist = Join-Path $Root "dist\bluey-windows-x64"
if (Test-Path $Dist) {
  Remove-Item -Recurse -Force $Dist
}
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

Copy-Item target\release\bluey.exe (Join-Path $Dist "bluey.exe")
Copy-Item target\release\bluey-daemon.exe (Join-Path $Dist "bluey-daemon.exe")
Copy-Item target\release\bluey-daemon.exe (Join-Path $Dist "termb.exe")
Copy-Item target\release\bluey-daemon.exe (Join-Path $Dist "Terminal.exe")
Copy-Item target\release\cue.exe (Join-Path $Dist "cue.exe")
Copy-Item target\release\cue-daemon.exe (Join-Path $Dist "cue-daemon.exe")
Copy-Item native\windows\cue-overlay\build\bluey-overlay.exe (Join-Path $Dist "bluey-overlay.exe")
Copy-Item native\windows\cue-overlay\build\bluey-overlay.exe (Join-Path $Dist "hostovb.exe")
Copy-Item native\windows\cue-overlay\build\bluey-overlay.exe (Join-Path $Dist "host-overlay.exe")
Copy-Item native\windows\cue-overlay\build\cue-overlay.exe (Join-Path $Dist "cue-overlay.exe")
Copy-Item native\windows\cue-audio\build\bluey-audio.exe (Join-Path $Dist "bluey-audio.exe")
Copy-Item native\windows\cue-audio\build\bluey-audio.exe (Join-Path $Dist "adriverb.exe")
Copy-Item native\windows\cue-audio\build\bluey-audio.exe (Join-Path $Dist "audio-driver.exe")
Copy-Item native\windows\cue-audio\build\cue-audio.exe (Join-Path $Dist "cue-audio.exe")

Write-Output $Dist
