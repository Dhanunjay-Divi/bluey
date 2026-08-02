param(
  [Parameter(Mandatory = $true)]
  [string]$BinDir,
  [string]$BuildId = $env:BLUEY_BUILD_ID
)

$ErrorActionPreference = "Stop"
$ResolvedBin = (Resolve-Path $BinDir).Path
$Entries = @()
Get-ChildItem -Path $ResolvedBin -File |
  Where-Object { $_.Name -ne "bluey-integrity.json" } |
  Sort-Object Name |
  ForEach-Object {
    $Entries += [ordered]@{
      path = $_.Name
      sha256 = (Get-FileHash -Algorithm SHA256 -Path $_.FullName).Hash.ToLowerInvariant()
      size_bytes = $_.Length
    }
  }

if ($Entries.Count -gt 64) {
  throw "Bluey Windows integrity manifest supports at most 64 files"
}

$Manifest = [ordered]@{
  schema_version = 1
  product = "Bluey"
  build_id = if ([string]::IsNullOrWhiteSpace($BuildId)) { "unknown" } else { $BuildId }
  files = $Entries
}
$ManifestPath = Join-Path $ResolvedBin "bluey-integrity.json"
$Json = ($Manifest | ConvertTo-Json -Depth 4) + "`n"
[System.IO.File]::WriteAllText(
  $ManifestPath,
  $Json,
  [System.Text.UTF8Encoding]::new($false)
)
$Written = [System.IO.File]::ReadAllBytes($ManifestPath)
if ($Written.Length -ge 3 -and $Written[0] -eq 0xEF -and $Written[1] -eq 0xBB -and $Written[2] -eq 0xBF) {
  throw "Bluey integrity manifest must be UTF-8 without BOM"
}
$RoundTrip = [System.IO.File]::ReadAllText($ManifestPath) | ConvertFrom-Json
if ($RoundTrip.schema_version -ne 1 -or $RoundTrip.product -ne "Bluey") {
  throw "Bluey integrity manifest failed its JSON round-trip check"
}
