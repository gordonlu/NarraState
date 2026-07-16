$ErrorActionPreference = 'Stop'
$appRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $appRoot
New-Item -ItemType Directory -Force -Path 'data' | Out-Null

$database = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { 'data/narrastate.db' }
$port = if ($env:NARRASTATE_PORT) { $env:NARRASTATE_PORT } else { '3000' }
Write-Host "谜局AI 正在启动：http://127.0.0.1:$port"
Write-Host '首次使用请在网页“设置”中填写模型服务，不要把 API Key 写入此脚本。'
& (Join-Path $appRoot 'narrastate-server.exe') serve --db $database --cases cases --web web
