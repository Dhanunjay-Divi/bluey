# Bluey one-line installer for Windows.
#
# Usage:
#   irm https://bluey.sh/install.ps1 | iex
#
# Installs Bluey for the current user into:
#   %LOCALAPPDATA%\Bluey\bin
#
# The update path runs this script only after the CLI verifies latest.json.sig.
# Direct installs use the HTTPS-delivered bootstrap script as their trust root
# and verify the artifact SHA256 supplied by that same origin. Updates are
# stronger: the installed CLI verifies latest.json.sig before handing the
# manifest-pinned artifact SHA256 to this script.

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$DownloadHost = if (![string]::IsNullOrWhiteSpace($env:BLUEY_DOWNLOAD_HOST)) {
    $env:BLUEY_DOWNLOAD_HOST.TrimEnd("/")
} else {
    "https://bluey.sh"
}
$Version = if (![string]::IsNullOrWhiteSpace($env:BLUEY_VERSION)) {
    $env:BLUEY_VERSION
} else {
    "latest"
}
$InstallRoot = if (![string]::IsNullOrWhiteSpace($env:BLUEY_INSTALL_ROOT)) {
    $env:BLUEY_INSTALL_ROOT
} else {
    Join-Path $env:LOCALAPPDATA "Bluey"
}
$BinDir = Join-Path $InstallRoot "bin"
$Platform = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64" -or $env:PROCESSOR_ARCHITEW6432 -eq "ARM64") {
    "windows-arm64"
} else {
    "windows-x86_64"
}
$InstallContext = if ([string]::IsNullOrWhiteSpace($env:BLUEY_INSTALL_CONTEXT)) {
    "direct"
} else {
    $env:BLUEY_INSTALL_CONTEXT
}

function Write-Step {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Cyan
}

function Write-Ok {
    param([string]$Message)
    Write-Host "OK  $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "WARN  $Message" -ForegroundColor Yellow
}

function Fail {
    param([string]$Message)
    Write-Host "ERROR  $Message" -ForegroundColor Red
    throw $Message
}

$DownloadUri = [Uri]$DownloadHost
if ($DownloadUri.Scheme -ne "https" -and $env:BLUEY_INSTALL_ALLOW_INSECURE_HOST -ne "1") {
    Fail "Bluey installation requires HTTPS. Set BLUEY_INSTALL_ALLOW_INSECURE_HOST=1 only for an isolated development host."
}
if ($InstallContext -eq "update" -and [string]::IsNullOrWhiteSpace($env:BLUEY_ARTIFACT_SHA256)) {
    Fail "Signed update handoff did not provide the manifest-pinned artifact SHA256"
}

function Resolve-BlueyUrl {
    param(
        [string]$Base,
        [string]$Value
    )
    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $null
    }
    if ($Value -match '^https?://') {
        return $Value
    }
    $baseUri = [Uri]::new(($Base.TrimEnd("/") + "/latest.json"))
    return ([Uri]::new($baseUri, $Value)).AbsoluteUri
}

function Get-FileSha256 {
    param([string]$Path)
    return (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLowerInvariant()
}

function Assert-FileSha256 {
    param(
        [string]$Path,
        [string]$Expected
    )
    if ([string]::IsNullOrWhiteSpace($Expected)) {
        Fail "Missing SHA256 for downloaded Bluey artifact"
    }
    $expectedTrimmed = ($Expected -split '\s+')[0].Trim().ToLowerInvariant()
    $actual = Get-FileSha256 -Path $Path
    if ($actual -ne $expectedTrimmed) {
        Fail "Checksum mismatch for $(Split-Path -Leaf $Path). Expected $expectedTrimmed, got $actual"
    }
}

function Assert-BlueyBinIntegrity {
    param([string]$Dir)

    $manifestPath = Join-Path $Dir "bluey-integrity.json"
    if (!(Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        Fail "Bluey archive did not contain bin\bluey-integrity.json"
    }
    if ((Get-Item -LiteralPath $manifestPath).Length -gt 262144) {
        Fail "Bluey integrity manifest is oversized"
    }
    try {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    } catch {
        Fail "Bluey integrity manifest is invalid JSON"
    }
    if ($manifest.schema_version -ne 1 -or $manifest.product -ne "Bluey") {
        Fail "Bluey integrity manifest has an unsupported schema or product"
    }
    $entries = @($manifest.files)
    if ($entries.Count -lt 3 -or $entries.Count -gt 64) {
        Fail "Bluey integrity manifest has an invalid file count"
    }
    $seen = @{}
    foreach ($entry in $entries) {
        $name = [string]$entry.path
        if ([string]::IsNullOrWhiteSpace($name) -or
            $name -ne [System.IO.Path]::GetFileName($name) -or
            $name.Contains("/") -or $name.Contains("\")) {
            Fail "Bluey integrity manifest contains an unsafe path"
        }
        $key = $name.ToLowerInvariant()
        if ($seen.ContainsKey($key)) {
            Fail "Bluey integrity manifest contains a duplicate path"
        }
        $seen[$key] = $true
        $expected = ([string]$entry.sha256).ToLowerInvariant()
        if ($expected -notmatch '^[0-9a-f]{64}$') {
            Fail "Bluey integrity manifest contains an invalid SHA256"
        }
        $path = Join-Path $Dir $name
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) {
            Fail "Bluey package file is missing: $name"
        }
        $item = Get-Item -LiteralPath $path
        if ([uint64]$item.Length -ne [uint64]$entry.size_bytes) {
            Fail "Bluey package file size mismatch: $name"
        }
        Assert-FileSha256 -Path $path -Expected $expected
    }
    foreach ($required in @("bluey.exe", "bluey-daemon.exe", "host-overlay.exe", "audio-driver.exe", "screen-driver.exe", "BLUEY-NOTICE.txt")) {
        if (!$seen.ContainsKey($required.ToLowerInvariant())) {
            Fail "Bluey integrity manifest is missing required file: $required"
        }
    }
    if (!$seen.ContainsKey("terminal.exe")) {
        Fail "Bluey integrity manifest is missing required process alias: Terminal.exe"
    }
    Get-ChildItem -LiteralPath $Dir -File |
        Where-Object { $_.Name -ne "bluey-integrity.json" } |
        ForEach-Object {
            if (!$seen.ContainsKey($_.Name.ToLowerInvariant())) {
                Fail "Bluey package contains an unmanifested file: $($_.Name)"
            }
        }
    if (Get-ChildItem -LiteralPath $Dir -Directory | Select-Object -First 1) {
        Fail "Bluey Windows bin package must not contain unmanifested directories"
    }
}

function Protect-BlueyBinAcl {
    param([string]$Dir)

    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    if ([string]::IsNullOrWhiteSpace($sid)) {
        Fail "Could not determine the current Windows user SID"
    }
    & icacls.exe $Dir /inheritance:r /grant:r "*$sid`:(OI)(CI)F" /T /C /Q | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Fail "Could not apply owner-only permissions to the Bluey bin directory"
    }
}

function Invoke-BlueyInstallCanary {
    param([string]$Dir)

    $bluey = Join-Path $Dir "bluey.exe"
    $daemon = Join-Path $Dir "bluey-daemon.exe"
    $legal = & $bluey legal --json 2>&1
    if ($LASTEXITCODE -ne 0 -or ($legal -join "`n") -notmatch 'LicenseRef-Bluey-Proprietary') {
        throw "bluey.exe legal canary failed"
    }
    & $daemon --help *> $null
    if ($LASTEXITCODE -ne 0) {
        throw "bluey-daemon.exe startup canary failed"
    }
}

function Refresh-UserPath {
    $machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ([string]::IsNullOrWhiteSpace($machinePath)) {
        $env:Path = $userPath
    } elseif ([string]::IsNullOrWhiteSpace($userPath)) {
        $env:Path = $machinePath
    } else {
        $env:Path = "$machinePath;$userPath"
    }
}

function Ensure-UserPathEntry {
    param([string]$Dir)
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if (![string]::IsNullOrWhiteSpace($userPath)) {
        $parts = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    }
    if ($parts -notcontains $Dir) {
        $parts = @($Dir) + $parts
        [Environment]::SetEnvironmentVariable("Path", (($parts | Select-Object -Unique) -join ';'), "User")
    }
    Refresh-UserPath
}

function Stop-BlueyForInstall {
    Get-Process -Name "bluey", "bluey-daemon", "cue", "cue-daemon", "bluey-overlay", "cue-overlay", "bluey-audio", "cue-audio" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue

    $installRootFull = [System.IO.Path]::GetFullPath($InstallRoot).TrimEnd('\')
    Get-Process -Name "Terminal", "host-overlay", "audio-driver", "screen-driver" -ErrorAction SilentlyContinue |
        Where-Object {
            try {
                $processPath = $_.Path
                if ([string]::IsNullOrWhiteSpace($processPath)) {
                    $false
                } else {
                    [System.IO.Path]::GetFullPath($processPath).StartsWith($installRootFull, [System.StringComparison]::OrdinalIgnoreCase)
                }
            } catch {
                $false
            }
        } |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

function Install-BlueyLocalDocTools {
    param([string]$Root)

    if ($env:BLUEY_SKIP_LOCAL_TOOLS -eq "1") {
        Write-Warn "Skipping Bluey document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
        return
    }

    $pythonCommand = Get-Command python3 -ErrorAction SilentlyContinue
    $pythonArgs = @()
    if (-not $pythonCommand) {
        $pythonCommand = Get-Command python -ErrorAction SilentlyContinue
    }
    if (-not $pythonCommand) {
        $pythonCommand = Get-Command py -ErrorAction SilentlyContinue
        if ($pythonCommand) {
            $pythonArgs = @("-3")
        }
    }
    if (-not $pythonCommand) {
        Write-Warn "Python was not found; document conversion will use built-in fallbacks only"
        return
    }

    Write-Step "Installing Bluey document tools..."
    $toolsDir = Join-Path $Root "tools\doc-converter"
    $venvDir = Join-Path $toolsDir ".venv"
    $wrapper = Join-Path (Join-Path $Root "bin") "bluey-doc-converter.cmd"
    New-Item -ItemType Directory -Force -Path $toolsDir, (Split-Path -Parent $wrapper) | Out-Null

    try {
        & $pythonCommand.Source @pythonArgs -m venv $venvDir | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
            return
        }

        $venvPython = Join-Path $venvDir "Scripts\python.exe"
        & $venvPython -m pip install --disable-pip-version-check --upgrade pip | Out-Null
        & $venvPython -m pip install --disable-pip-version-check "markitdown[all]" | Out-Null
        if ($LASTEXITCODE -ne 0) {
            & $venvPython -m pip install --disable-pip-version-check markitdown | Out-Null
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "Could not install MarkItDown for Bluey document tools; document conversion will use built-in fallbacks only"
            return
        }

        $wrapperLines = @(
            "@echo off",
            "set `"ROOT=%~dp0..`"",
            "`"%ROOT%\tools\doc-converter\.venv\Scripts\markitdown.exe`" %*"
        )
        Set-Content -Path $wrapper -Encoding ASCII -Value $wrapperLines
        Write-Ok "Bluey document tools installed"
    } catch {
        Write-Warn "Could not install Bluey document tools: $($_.Exception.Message)"
    }
}

function Get-RemoteText {
    param([string]$Uri)
    return (Invoke-WebRequest -Uri $Uri -UseBasicParsing).Content
}

Write-Host ""
Write-Step "Installing Bluey for Windows"
Write-Host "  Install path: $BinDir" -ForegroundColor DarkGray

$Latest = $null
if ($Version -eq "latest" -or [string]::IsNullOrWhiteSpace($env:BLUEY_ARTIFACT_URL)) {
    Write-Step "Resolving latest Bluey release..."
    $Latest = Invoke-RestMethod -Uri "$DownloadHost/latest.json"
    if ($Version -eq "latest") {
        if ([string]::IsNullOrWhiteSpace($Latest.version)) {
            Fail "latest.json did not include a version"
        }
        $Version = $Latest.version
    } elseif ($Latest.version.TrimStart("v") -ne $Version.TrimStart("v")) {
        # Never use mutable latest metadata for a caller-pinned older release.
        $Latest = $null
    }
}
if ($Platform -eq "windows-arm64" -and
    (!$Latest -or !$Latest.platforms -or !$Latest.platforms.PSObject.Properties["windows-arm64"])) {
    Write-Warn "Native Windows ARM64 is not published for this release; using the x64 compatibility build"
    $Platform = "windows-x86_64"
}

$VersionNumber = $Version.TrimStart("v")
$VersionTag = "v$VersionNumber"
$ArtifactUrl = $env:BLUEY_ARTIFACT_URL
$ArtifactSha256 = $env:BLUEY_ARTIFACT_SHA256

if ([string]::IsNullOrWhiteSpace($ArtifactUrl)) {
    $platformEntry = $null
    if ($Latest -and $Latest.platforms) {
        $platformProperty = $Latest.platforms.PSObject.Properties[$Platform]
        if ($platformProperty) {
            $platformEntry = $platformProperty.Value
        }
    }
    if ($platformEntry -and ![string]::IsNullOrWhiteSpace($platformEntry.url)) {
        $ArtifactUrl = Resolve-BlueyUrl -Base $DownloadHost -Value $platformEntry.url
        $ArtifactSha256 = $platformEntry.sha256
    } else {
        $ArtifactUrl = "$DownloadHost/releases/$VersionTag/bluey-$VersionNumber-$Platform.zip"
    }
}

$ArtifactUri = [Uri]$ArtifactUrl
if ($ArtifactUri.Scheme -ne "https" -and $env:BLUEY_INSTALL_ALLOW_INSECURE_HOST -ne "1") {
    Fail "Bluey artifact downloads require HTTPS"
}
$ArchiveName = Split-Path -Leaf $ArtifactUri.AbsolutePath
if ([string]::IsNullOrWhiteSpace($ArchiveName)) {
    $ArchiveName = "bluey-$VersionNumber-$Platform.zip"
}

$TempRoot = Join-Path $env:TEMP ("bluey-install-" + [guid]::NewGuid().ToString("N"))
$ZipPath = Join-Path $TempRoot $ArchiveName
$ExtractPath = Join-Path $TempRoot "extract"
New-Item -ItemType Directory -Force -Path $TempRoot, $ExtractPath | Out-Null

try {
    Write-Step "Downloading Bluey ($VersionTag, $Platform)..."
    Invoke-WebRequest -Uri $ArtifactUrl -OutFile $ZipPath -UseBasicParsing
    Write-Ok ("Downloaded {0:N0} bytes" -f ((Get-Item $ZipPath).Length))

    if ($env:BLUEY_SKIP_CHECKSUM -ne "1") {
        if ([string]::IsNullOrWhiteSpace($ArtifactSha256)) {
            $sumsUrl = "$DownloadHost/releases/$VersionTag/SHA256SUMS.txt"
            $sums = Get-RemoteText -Uri $sumsUrl
            $line = ($sums -split "`n" | Where-Object { $_ -match [regex]::Escape($ArchiveName) } | Select-Object -First 1)
            if (![string]::IsNullOrWhiteSpace($line)) {
                $ArtifactSha256 = ($line -split '\s+')[0]
            }
        }
        Assert-FileSha256 -Path $ZipPath -Expected $ArtifactSha256
        if ($InstallContext -eq "update") {
            Write-Ok "Signed-manifest artifact checksum verified"
        } else {
            Write-Ok "HTTPS-bootstrap artifact checksum verified"
        }
    } else {
        if ($InstallContext -eq "update" -or $env:BLUEY_INSTALL_ALLOW_INSECURE_HOST -ne "1") {
            Fail "BLUEY_SKIP_CHECKSUM is restricted to isolated development installs"
        }
        Write-Warn "Skipping checksum because BLUEY_SKIP_CHECKSUM=1"
    }

    Write-Step "Extracting Bluey..."
    Expand-Archive -Path $ZipPath -DestinationPath $ExtractPath -Force
    $ExtractedBin = Join-Path $ExtractPath "bin"
    if (!(Test-Path (Join-Path $ExtractedBin "bluey.exe"))) {
        Fail "Archive did not contain bin\bluey.exe"
    }
    if (!(Test-Path (Join-Path $ExtractedBin "bluey-daemon.exe"))) {
        Fail "Archive did not contain bin\bluey-daemon.exe"
    }
    Assert-BlueyBinIntegrity -Dir $ExtractedBin

    Stop-BlueyForInstall
    New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
    $StagedBin = Join-Path $InstallRoot ("bin.stage." + [guid]::NewGuid().ToString("N"))
    $PreviousBin = Join-Path $InstallRoot "bin.previous"
    Copy-Item -Path $ExtractedBin -Destination $StagedBin -Recurse -Force
    Assert-BlueyBinIntegrity -Dir $StagedBin
    Protect-BlueyBinAcl -Dir $StagedBin
    Get-ChildItem -Path $StagedBin -Filter "*.exe" -Recurse -ErrorAction SilentlyContinue |
        ForEach-Object { Unblock-File -Path $_.FullName -ErrorAction SilentlyContinue }
    $MovedPrevious = $false
    try {
        Remove-Item -LiteralPath $PreviousBin -Recurse -Force -ErrorAction SilentlyContinue
        if (Test-Path -LiteralPath $BinDir) {
            Move-Item -LiteralPath $BinDir -Destination $PreviousBin
            $MovedPrevious = $true
        }
        Move-Item -LiteralPath $StagedBin -Destination $BinDir
        Assert-BlueyBinIntegrity -Dir $BinDir
        Invoke-BlueyInstallCanary -Dir $BinDir
        if (Test-Path -LiteralPath $PreviousBin) {
            Remove-Item -LiteralPath $PreviousBin -Recurse -Force -ErrorAction Stop
        }
    } catch {
        $installError = $_
        Remove-Item -LiteralPath $BinDir -Recurse -Force -ErrorAction SilentlyContinue
        if ($MovedPrevious -and (Test-Path -LiteralPath $PreviousBin)) {
            Move-Item -LiteralPath $PreviousBin -Destination $BinDir
        }
        throw "Bluey install canary failed; previous version restored. $($installError.Exception.Message)"
    } finally {
        Remove-Item -LiteralPath $StagedBin -Recurse -Force -ErrorAction SilentlyContinue
    }
    Install-BlueyLocalDocTools -Root $InstallRoot

    Ensure-UserPathEntry -Dir $BinDir

    Write-Ok "Installed Bluey to $BinDir"
    Write-Ok "Added Bluey to your user PATH"
    Write-Host ""
    Write-Host "Bluey installed." -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  Run: bluey on" -ForegroundColor White
    Write-Host "  Remove later: bluey uninstall" -ForegroundColor White
    Write-Host "  Bluey will check for signed updates before starting." -ForegroundColor DarkGray
    Write-Host ""
} finally {
    Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
