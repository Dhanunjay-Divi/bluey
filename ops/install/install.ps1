# Bluey one-line installer for Windows.
#
# Usage:
#   irm https://bluey.sh/install.ps1 | iex
#
# Installs Bluey for the current user into:
#   %LOCALAPPDATA%\Bluey\bin
#
# The update path runs this script only after the CLI verifies latest.json.sig.
# Direct installs still verify the downloaded Windows artifact SHA256 from
# latest.json or the release SHA256SUMS.txt.

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
$Platform = "windows-x86_64"

if ($env:BLUEY_WINDOWS_INSTALL_PREVIEW -ne "1") {
    Write-Host ""
    Write-Host "Bluey for Windows is coming soon." -ForegroundColor Cyan
    Write-Host "The current public alpha installer is macOS-only while Windows signed artifacts finish validation." -ForegroundColor Gray
    Write-Host "Use macOS for this alpha, or check https://bluey.sh/download for the latest status." -ForegroundColor Gray
    exit 1
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
    exit 1
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
    Get-Process -Name "bluey", "bluey-daemon", "cue", "cue-daemon" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue
}

function Install-BlueyLocalDocTools {
    param([string]$Root)

    if ($env:BLUEY_SKIP_LOCAL_TOOLS -eq "1") {
        Write-Warn "Skipping Bluey-local document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
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

    Write-Step "Installing Bluey-local document tools..."
    $toolsDir = Join-Path $Root "tools\doc-converter"
    $venvDir = Join-Path $toolsDir ".venv"
    $wrapper = Join-Path (Join-Path $Root "bin") "bluey-doc-converter.cmd"
    New-Item -ItemType Directory -Force -Path $toolsDir, (Split-Path -Parent $wrapper) | Out-Null

    try {
        & $pythonCommand.Source @pythonArgs -m venv $venvDir | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "Could not create Bluey-local Python venv; document conversion will use built-in fallbacks only"
            return
        }

        $venvPython = Join-Path $venvDir "Scripts\python.exe"
        & $venvPython -m pip install --disable-pip-version-check --upgrade pip | Out-Null
        & $venvPython -m pip install --disable-pip-version-check "markitdown[all]" | Out-Null
        if ($LASTEXITCODE -ne 0) {
            & $venvPython -m pip install --disable-pip-version-check markitdown | Out-Null
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "Could not install MarkItDown into Bluey's local tools venv; document conversion will use built-in fallbacks only"
            return
        }

        $wrapperLines = @(
            "@echo off",
            "set `"ROOT=%~dp0..`"",
            "`"%ROOT%\tools\doc-converter\.venv\Scripts\markitdown.exe`" %*"
        )
        Set-Content -Path $wrapper -Encoding ASCII -Value $wrapperLines
        Write-Ok "Bluey-local document tools installed"
    } catch {
        Write-Warn "Could not install Bluey-local document tools: $($_.Exception.Message)"
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
    }
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
        Write-Ok "Checksum verified"
    } else {
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

    Stop-BlueyForInstall
    New-Item -ItemType Directory -Force -Path $InstallRoot | Out-Null
    Remove-Item -LiteralPath $BinDir -Recurse -Force -ErrorAction SilentlyContinue
    Copy-Item -Path $ExtractedBin -Destination $BinDir -Recurse -Force
    Get-ChildItem -Path $BinDir -Filter "*.exe" -Recurse -ErrorAction SilentlyContinue |
        ForEach-Object { Unblock-File -Path $_.FullName -ErrorAction SilentlyContinue }
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
