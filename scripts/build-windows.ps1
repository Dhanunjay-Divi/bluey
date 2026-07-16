$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

$CargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $CargoBin) {
  $env:Path = "$CargoBin;$env:Path"
}

function Invoke-CheckedCommand {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Name,
    [Parameter(Mandatory = $true)]
    [scriptblock]$Command
  )

  Write-Host "Building $Name..."
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "$Name failed with exit code $LASTEXITCODE."
  }
}

function Assert-BuildOutput {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Name,
    [Parameter(Mandatory = $true)]
    [string[]]$Paths
  )

  foreach ($Path in $Paths) {
    $Resolved = Join-Path $Root $Path
    if (-not (Test-Path -LiteralPath $Resolved -PathType Leaf)) {
      throw "$Name did not produce required output: $Path"
    }
  }
}

Invoke-CheckedCommand "Rust release binaries" { cargo build --release }
Invoke-CheckedCommand "Windows overlay" {
  powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File native\windows\cue-overlay\build.ps1
}
Assert-BuildOutput "Windows overlay" @(
  "native\windows\cue-overlay\build\bluey-overlay.exe",
  "native\windows\cue-overlay\build\cue-overlay.exe",
  "native\windows\cue-overlay\build\hostovb.exe",
  "native\windows\cue-overlay\build\host-overlay.exe"
)

Invoke-CheckedCommand "Windows screen capture helper" {
  powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File native\windows\cue-capture\build.ps1
}
Assert-BuildOutput "Windows screen capture helper" @(
  "native\windows\cue-capture\build\bluey-capture.exe",
  "native\windows\cue-capture\build\cue-capture.exe",
  "native\windows\cue-capture\build\screen-driver.exe"
)

Invoke-CheckedCommand "Windows audio driver" {
  powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
    -File native\windows\cue-audio\build.ps1
}
Assert-BuildOutput "Windows audio driver" @(
  "native\windows\cue-audio\build\bluey-audio.exe",
  "native\windows\cue-audio\build\cue-audio.exe",
  "native\windows\cue-audio\build\adriverb.exe",
  "native\windows\cue-audio\build\audio-driver.exe"
)

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
Copy-Item native\windows\cue-capture\build\bluey-capture.exe (Join-Path $Dist "bluey-capture.exe")
Copy-Item native\windows\cue-capture\build\cue-capture.exe (Join-Path $Dist "cue-capture.exe")
Copy-Item native\windows\cue-capture\build\screen-driver.exe (Join-Path $Dist "screen-driver.exe")
Copy-Item native\windows\cue-audio\build\bluey-audio.exe (Join-Path $Dist "bluey-audio.exe")
Copy-Item native\windows\cue-audio\build\bluey-audio.exe (Join-Path $Dist "adriverb.exe")
Copy-Item native\windows\cue-audio\build\bluey-audio.exe (Join-Path $Dist "audio-driver.exe")
Copy-Item native\windows\cue-audio\build\cue-audio.exe (Join-Path $Dist "cue-audio.exe")
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
