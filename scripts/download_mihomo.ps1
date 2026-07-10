param(
    [string]$OutputDirectory = "src-tauri/resources",
    [string]$MihomoVersion = "v1.19.28"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$repo = "MetaCubeX/mihomo"
$api = "https://api.github.com/repos/$repo/releases/tags/$MihomoVersion"
$headers = @{
    "Accept" = "application/vnd.github+json"
    "User-Agent" = "YulongVPN-Windows-Build"
    "X-GitHub-Api-Version" = "2022-11-28"
}
if ($env:GITHUB_TOKEN) {
    $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
}

Write-Host "Reading pinned official mihomo release $MihomoVersion..."
$release = Invoke-RestMethod -Uri $api -Headers $headers

$patterns = @(
    '^mihomo-windows-amd64-compatible-v.*\.zip$',
    '^mihomo-windows-amd64-v.*\.zip$',
    '^mihomo-windows-amd64-v3-v.*\.zip$',
    'windows-amd64.*\.zip$'
)

$asset = $null
foreach ($pattern in $patterns) {
    $asset = $release.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
    if ($asset) { break }
}
if (-not $asset) {
    $available = ($release.assets | ForEach-Object { $_.name }) -join "`n"
    throw "No compatible Windows amd64 mihomo ZIP found. Assets:`n$available"
}

$root = Resolve-Path "."
$output = Join-Path $root $OutputDirectory
New-Item -ItemType Directory -Force -Path $output | Out-Null
$temp = Join-Path $env:RUNNER_TEMP "yulong-mihomo"
if (-not $env:RUNNER_TEMP) {
    $temp = Join-Path $env:TEMP "yulong-mihomo"
}
Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $temp | Out-Null

$zip = Join-Path $temp $asset.name
Write-Host "Downloading $($asset.name) from the official MetaCubeX/mihomo release..."
Invoke-WebRequest -Uri $asset.browser_download_url -Headers $headers -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $temp -Force

$exe = Get-ChildItem -Path $temp -Recurse -File -Filter "*.exe" |
    Where-Object { $_.Name -match '^mihomo.*\.exe$' } |
    Select-Object -First 1
if (-not $exe) {
    throw "The downloaded archive did not contain mihomo.exe"
}

$coreTarget = Join-Path $output "mihomo.exe"
Copy-Item $exe.FullName $coreTarget -Force
Set-Content -Path (Join-Path $output "mihomo-version.txt") -Value $release.tag_name -Encoding UTF8

$licenseUrl = "https://raw.githubusercontent.com/MetaCubeX/mihomo/main/LICENSE"
$licenseTarget = Join-Path $output "THIRD_PARTY_LICENSES.txt"
$notice = @"
玉龙VPN Windows 内置开源代理核心：mihomo
来源：https://github.com/MetaCubeX/mihomo
版本：$($release.tag_name)
发布资产：$($asset.name)

以下为 mihomo 上游许可证：

"@
Set-Content -Path $licenseTarget -Value $notice -Encoding UTF8
try {
    $license = Invoke-WebRequest -Uri $licenseUrl -Headers $headers
    Add-Content -Path $licenseTarget -Value $license.Content -Encoding UTF8
} catch {
    Add-Content -Path $licenseTarget -Value "许可证下载失败，请访问上游仓库查看。" -Encoding UTF8
}

$versionOutput = & $coreTarget -v 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    throw "mihomo core validation failed: $versionOutput"
}

$coreSize = (Get-Item $coreTarget).Length
if ($coreSize -lt 5MB) {
    throw "mihomo core file is unexpectedly small: $coreSize bytes"
}

Write-Host "mihomo ready: $versionOutput"
Write-Host "core path: $coreTarget"
Write-Host "core bytes: $coreSize"
