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
$BlueyUvVersion = "0.11.29"
$BlueyMarkItDownVersion = "0.1.6"
$BlueyMarkItDownExcludeNewer = "2026-07-16T00:00:00Z"
$BlueyAzureContentUnderstandingVersion = "1.2.0b2"

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
    if ($expectedTrimmed -notmatch '^[0-9a-f]{64}$') {
        Fail "Invalid SHA256 for downloaded Bluey artifact"
    }
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
    Get-Process -Name "bluey", "bluey-daemon", "cue", "cue-daemon", "bluey-overlay", "cue-overlay", "bluey-audio", "cue-audio", "bluey-capture", "cue-capture" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue

    $installRootFull = [System.IO.Path]::GetFullPath($InstallRoot).TrimEnd('\')
    Get-Process -Name "termb", "Terminal", "hostovb", "host-overlay", "adriverb", "audio-driver", "screen-driver" -ErrorAction SilentlyContinue |
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

function Copy-FirstBinaryAlias {
    param(
        [string]$Dir,
        [string]$AliasName,
        [string[]]$Candidates
    )
    $aliasPath = Join-Path $Dir $AliasName
    if (Test-Path $aliasPath) {
        return
    }
    foreach ($candidate in $Candidates) {
        $candidatePath = Join-Path $Dir $candidate
        if (Test-Path $candidatePath) {
            Copy-Item -Force $candidatePath $aliasPath
            return
        }
    }
}

function Replace-FirstBinaryAlias {
    param(
        [string]$Dir,
        [string]$AliasName,
        [string[]]$Candidates
    )
    $aliasPath = Join-Path $Dir $AliasName
    Remove-Item -LiteralPath $aliasPath -Force -ErrorAction SilentlyContinue
    foreach ($candidate in $Candidates) {
        $candidatePath = Join-Path $Dir $candidate
        if (Test-Path $candidatePath) {
            Copy-Item -Force $candidatePath $aliasPath
            return
        }
    }
}

function Ensure-ProcessIdentityAliases {
    param([string]$Dir)

    Replace-FirstBinaryAlias -Dir $Dir -AliasName "termb.exe" -Candidates @("bluey-daemon.exe", "cue-daemon.exe")
    Copy-FirstBinaryAlias -Dir $Dir -AliasName "Terminal.exe" -Candidates @("bluey-daemon.exe", "cue-daemon.exe")
    Copy-FirstBinaryAlias -Dir $Dir -AliasName "hostovb.exe" -Candidates @("bluey-overlay.exe", "cue-overlay.exe")
    Copy-FirstBinaryAlias -Dir $Dir -AliasName "host-overlay.exe" -Candidates @("bluey-overlay.exe", "cue-overlay.exe")
    Copy-FirstBinaryAlias -Dir $Dir -AliasName "adriverb.exe" -Candidates @("bluey-audio.exe", "cue-audio.exe")
    Copy-FirstBinaryAlias -Dir $Dir -AliasName "audio-driver.exe" -Candidates @("bluey-audio.exe", "cue-audio.exe")
    Copy-FirstBinaryAlias -Dir $Dir -AliasName "screen-driver.exe" -Candidates @("bluey-capture.exe", "cue-capture.exe")
}

function Test-BlueyPythonCommand {
    param(
        [System.Management.Automation.CommandInfo]$Command,
        [string[]]$Args = @()
    )
    if (-not $Command) {
        return $false
    }
    try {
        $output = & $Command.Source @Args --version 2>&1
        if ($LASTEXITCODE -ne 0) {
            return $false
        }
        return (($output -join "`n") -match 'Python\s+3\.')
    } catch {
        return $false
    }
}

function Get-BlueyPythonCommand {
    $python3 = Get-Command python3 -ErrorAction SilentlyContinue
    if (Test-BlueyPythonCommand -Command $python3) {
        return @{ Source = $python3.Source; Args = @() }
    }

    $python = Get-Command python -ErrorAction SilentlyContinue
    if (Test-BlueyPythonCommand -Command $python) {
        return @{ Source = $python.Source; Args = @() }
    }

    $py = Get-Command py -ErrorAction SilentlyContinue
    if (Test-BlueyPythonCommand -Command $py -Args @("-3")) {
        return @{ Source = $py.Source; Args = @("-3") }
    }

    return $null
}

function Get-BlueyUvUrl {
    if (![string]::IsNullOrWhiteSpace($env:BLUEY_UV_URL)) {
        if ([string]::IsNullOrWhiteSpace($env:BLUEY_UV_SHA256)) {
            return $null
        }
        return $env:BLUEY_UV_URL
    }

    $arch = if (![string]::IsNullOrWhiteSpace($env:PROCESSOR_ARCHITEW6432)) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    switch -Regex ($arch) {
        '^(ARM64|AARCH64)$' {
            return "https://github.com/astral-sh/uv/releases/download/$BlueyUvVersion/uv-aarch64-pc-windows-msvc.zip"
        }
        default {
            return "https://github.com/astral-sh/uv/releases/download/$BlueyUvVersion/uv-x86_64-pc-windows-msvc.zip"
        }
    }
}

function Get-BlueyUvSha256 {
    if (![string]::IsNullOrWhiteSpace($env:BLUEY_UV_URL)) {
        if ([string]::IsNullOrWhiteSpace($env:BLUEY_UV_SHA256)) {
            return $null
        }
        return $env:BLUEY_UV_SHA256
    }

    $arch = if (![string]::IsNullOrWhiteSpace($env:PROCESSOR_ARCHITEW6432)) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    switch -Regex ($arch) {
        '^(ARM64|AARCH64)$' {
            return "55b597ae81bc29531a7c352a1431a8a73cc2755d7a5b9ec454580cbe02e5154f"
        }
        default {
            return "a047d55651bc3e0ca24595b25ec4cfcb10f9dca9fb56514e661269b37d4fae68"
        }
    }
}

function Install-BlueyLocalUv {
    param([string]$Root)

    if (
        ![string]::IsNullOrWhiteSpace($env:BLUEY_UV_URL) -and
        [string]::IsNullOrWhiteSpace($env:BLUEY_UV_SHA256)
    ) {
        Write-Warn "BLUEY_UV_URL overrides require BLUEY_UV_SHA256"
        return $null
    }

    $uvDir = Join-Path $Root "tools\uv"
    $uvExe = Join-Path $uvDir "uv.exe"
    if (Test-Path $uvExe) {
        try {
            $installedVersion = (& $uvExe --version 2>$null) -replace '^uv\s+', ''
            $installedVersion = ($installedVersion -split '\s+')[0]
            if ($installedVersion -eq $BlueyUvVersion) {
                return $uvExe
            }
        } catch {
            # Replace an unreadable or stale helper below.
        }
        Remove-Item -LiteralPath $uvExe -Force -ErrorAction SilentlyContinue
    }

    $uvUrl = Get-BlueyUvUrl
    $uvSha256 = Get-BlueyUvSha256
    if ([string]::IsNullOrWhiteSpace($uvUrl) -or [string]::IsNullOrWhiteSpace($uvSha256)) {
        Write-Warn "BLUEY_UV_URL overrides require BLUEY_UV_SHA256"
        return $null
    }
    $tempRoot = Join-Path $env:TEMP ("bluey-uv-" + [guid]::NewGuid().ToString("N"))
    $zipPath = Join-Path $tempRoot "uv.zip"
    $extractPath = Join-Path $tempRoot "extract"

    try {
        New-Item -ItemType Directory -Force -Path $uvDir, $tempRoot, $extractPath | Out-Null
        Write-Step "Installing Bluey local Python runtime helper..."
        Invoke-WebRequest -Uri $uvUrl -OutFile $zipPath -UseBasicParsing
        $expectedUvSha256 = ($uvSha256 -split '\s+')[0].Trim().ToLowerInvariant()
        if ($expectedUvSha256 -notmatch '^[0-9a-f]{64}$') {
            throw "Bluey local Python runtime helper SHA256 must contain exactly 64 hexadecimal characters"
        }
        $actualUvSha256 = Get-FileSha256 -Path $zipPath
        if ($actualUvSha256 -ne $expectedUvSha256) {
            throw "Checksum mismatch for the Bluey local Python runtime helper. Expected $expectedUvSha256, got $actualUvSha256"
        }
        Expand-Archive -Path $zipPath -DestinationPath $extractPath -Force
        $downloadedUv = Get-ChildItem -Path $extractPath -Filter "uv.exe" -Recurse -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $downloadedUv) {
            Write-Warn "Could not find uv.exe in the downloaded Bluey Python helper"
            return $null
        }
        Copy-Item -Force $downloadedUv.FullName $uvExe
        Write-Ok "Installed Bluey local Python runtime helper"
        return $uvExe
    } catch {
        Write-Warn "Could not install Bluey local Python runtime helper: $($_.Exception.Message)"
        return $null
    } finally {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-WithBlueyUvEnv {
    param(
        [string]$Root,
        [scriptblock]$Body
    )

    $oldCache = $env:UV_CACHE_DIR
    $oldPythonInstall = $env:UV_PYTHON_INSTALL_DIR
    $oldDownloads = $env:UV_PYTHON_DOWNLOADS
    $oldLinkMode = $env:UV_LINK_MODE
    $exitCode = 0
    try {
        $env:UV_CACHE_DIR = Join-Path $Root "tools\uv-cache"
        $env:UV_PYTHON_INSTALL_DIR = Join-Path $Root "tools\python"
        $env:UV_PYTHON_DOWNLOADS = "automatic"
        $env:UV_LINK_MODE = "copy"
        New-Item -ItemType Directory -Force -Path $env:UV_CACHE_DIR, $env:UV_PYTHON_INSTALL_DIR | Out-Null
        & $Body
        $exitCode = $LASTEXITCODE
    } finally {
        if ($null -eq $oldCache) { Remove-Item Env:\UV_CACHE_DIR -ErrorAction SilentlyContinue } else { $env:UV_CACHE_DIR = $oldCache }
        if ($null -eq $oldPythonInstall) { Remove-Item Env:\UV_PYTHON_INSTALL_DIR -ErrorAction SilentlyContinue } else { $env:UV_PYTHON_INSTALL_DIR = $oldPythonInstall }
        if ($null -eq $oldDownloads) { Remove-Item Env:\UV_PYTHON_DOWNLOADS -ErrorAction SilentlyContinue } else { $env:UV_PYTHON_DOWNLOADS = $oldDownloads }
        if ($null -eq $oldLinkMode) { Remove-Item Env:\UV_LINK_MODE -ErrorAction SilentlyContinue } else { $env:UV_LINK_MODE = $oldLinkMode }
    }
    return $exitCode
}

function Install-BlueyLocalDocTools {
    param([string]$Root)

    $toolsDir = Join-Path $Root "tools\doc-converter"
    $venvDir = Join-Path $toolsDir ".venv"
    $wrapper = Join-Path (Join-Path $Root "bin") "bluey-doc-converter.cmd"
    New-Item -ItemType Directory -Force -Path $toolsDir, (Split-Path -Parent $wrapper) | Out-Null
    Remove-Item -LiteralPath $wrapper -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $venvDir -Recurse -Force -ErrorAction SilentlyContinue

    if ($env:BLUEY_SKIP_LOCAL_TOOLS -eq "1") {
        Write-Warn "Skipping Bluey document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
        return
    }

    Write-Step "Installing Bluey document tools..."

    try {
        $uvExe = Install-BlueyLocalUv -Root $Root
        if (-not $uvExe) {
            Write-Warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
            return
        }
        $uvExitCode = Invoke-WithBlueyUvEnv -Root $Root -Body {
            & $uvExe venv --python 3.12 $venvDir | Out-Null
        }
        if ($uvExitCode -ne 0) {
            Remove-Item -LiteralPath $venvDir -Recurse -Force -ErrorAction SilentlyContinue
            Write-Warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
            return
        }

        $venvPython = Join-Path $venvDir "Scripts\python.exe"
        $uvExitCode = Invoke-WithBlueyUvEnv -Root $Root -Body {
            & $uvExe pip install --prerelease explicit --python $venvPython --exclude-newer $BlueyMarkItDownExcludeNewer "markitdown[all]==$BlueyMarkItDownVersion" "azure-ai-contentunderstanding==$BlueyAzureContentUnderstandingVersion" | Out-Null
        }
        if ($uvExitCode -ne 0) {
            Remove-Item -LiteralPath $venvDir -Recurse -Force -ErrorAction SilentlyContinue
            $uvExitCode = Invoke-WithBlueyUvEnv -Root $Root -Body {
                & $uvExe venv --python 3.12 $venvDir | Out-Null
            }
            if ($uvExitCode -ne 0) {
                Remove-Item -LiteralPath $venvDir -Recurse -Force -ErrorAction SilentlyContinue
                Write-Warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
                return
            }
            $uvExitCode = Invoke-WithBlueyUvEnv -Root $Root -Body {
                & $uvExe pip install --python $venvPython --exclude-newer $BlueyMarkItDownExcludeNewer "markitdown==$BlueyMarkItDownVersion" | Out-Null
            }
        }
        if ($uvExitCode -ne 0) {
            Remove-Item -LiteralPath $venvDir -Recurse -Force -ErrorAction SilentlyContinue
            Write-Warn "Could not install pinned MarkItDown for Bluey document tools; document conversion will use built-in fallbacks only"
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
        Remove-Item -LiteralPath $wrapper -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $venvDir -Recurse -Force -ErrorAction SilentlyContinue
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
    } elseif ($Latest) {
        Fail "Bluey $VersionTag does not include a Windows download yet. No files were changed. Please try again after the Windows release is published."
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
    Ensure-ProcessIdentityAliases -Dir $BinDir
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
