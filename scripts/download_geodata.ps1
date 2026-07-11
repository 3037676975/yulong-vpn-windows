param(
    [string]$OutputDirectory = "src-tauri/resources"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$headers = @{
    "Accept" = "application/vnd.github+json"
    "User-Agent" = "YulongVPN-Windows-Build"
}
if ($env:GITHUB_TOKEN) {
    $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
}

$release = Invoke-RestMethod `
    -Uri "https://api.github.com/repos/MetaCubeX/meta-rules-dat/releases/latest" `
    -Headers $headers

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

foreach ($name in @("geoip.metadb", "geosite.dat")) {
    $asset = $release.assets | Where-Object { $_.name -eq $name } | Select-Object -First 1
    if (-not $asset) {
        $available = ($release.assets | ForEach-Object { $_.name }) -join ", "
        throw "Missing $name in MetaCubeX/meta-rules-dat release. Assets: $available"
    }

    $target = Join-Path $OutputDirectory $name
    Write-Host "Downloading bundled $name..."
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $target -Headers $headers
    if ((Get-Item $target).Length -lt 100KB) {
        throw "$name is unexpectedly small"
    }
    Write-Host "$name ready: $((Get-Item $target).Length) bytes"
}

Set-Content `
    -Path (Join-Path $OutputDirectory "geodata-version.txt") `
    -Value $release.tag_name `
    -Encoding UTF8
