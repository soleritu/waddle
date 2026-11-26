# Installs Waddle on Windows. Run: irm https://raw.githubusercontent.com/soleritu/waddle/main/install.ps1 | iex
$ErrorActionPreference = "Stop"
Write-Host "Finding the newest Waddle..."
$release = (Invoke-RestMethod "https://api.github.com/repos/soleritu/waddle/releases?per_page=5" -Headers @{"User-Agent" = "waddle-installer"}) | Where-Object { -not $_.draft } | Select-Object -First 1
$asset = $release.assets | Where-Object { $_.name -eq "Waddle-Setup.exe" } | Select-Object -First 1
if (-not $asset) { throw "The newest release has no Windows installer yet. Try again in a few minutes." }
$setup = Join-Path $env:TEMP "Waddle-Setup.exe"
Write-Host "Downloading Waddle $($release.tag_name)..."
Invoke-WebRequest $asset.browser_download_url -OutFile $setup -UseBasicParsing
Start-Process $setup -ArgumentList "/SILENT", "/SP-" -Wait
Remove-Item $setup
Write-Host "Done. Waddle is open, and it's in your Start menu."
