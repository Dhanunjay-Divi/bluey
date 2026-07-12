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
powershell -ExecutionPolicy Bypass -File native\windows\cue-whisper\build.ps1 | Out-Null

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
Copy-Item native\windows\cue-whisper\cue-whisper.exe (Join-Path $Dist "cue-whisper.exe")

$VersionLine = Select-String -Path (Join-Path $Root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $VersionLine) {
  throw "Could not determine Bluey version from Cargo.toml"
}
$Version = $VersionLine.Matches[0].Groups[1].Value
$Stage = Join-Path $Root "dist\staging-windows-x64"
$StageBin = Join-Path $Stage "bin"
$Archive = Join-Path $Root "dist\bluey-$Version-windows-x86_64.zip"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
Remove-Item -Force $Archive -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $StageBin | Out-Null
Copy-Item (Join-Path $Dist "*") $StageBin -Recurse -Force
Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Archive -CompressionLevel Optimal
Remove-Item -Recurse -Force $Stage

Write-Output $Dist
Write-Output $Archive
