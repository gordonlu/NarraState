$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

foreach ($commandName in @('npm', 'cargo')) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) {
        throw "缺少 $commandName。请先安装 Node.js 22 和 stable Rust，随后重新运行 .\start.ps1。"
    }
}

New-Item -ItemType Directory -Force -Path 'data' | Out-Null

Write-Host '[1/3] 安装前端依赖'
npm --prefix web ci
Write-Host '[2/3] 构建谜局AI Web 界面'
npm --prefix web run build
$database = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { 'data/narrastate.db' }
$port = if ($env:NARRASTATE_PORT) { $env:NARRASTATE_PORT } else { '3000' }
Write-Host '[3/3] 启动谜局AI'
Write-Host "打开 http://127.0.0.1:$port"
Write-Host '首次使用：进入“设置”，填写 OpenAI-compatible Base URL、模型和 API Key，然后测试并保存。'
Write-Host 'API Key 会保存在本机 data/provider.env；未配置时仍可使用 Mock 模式。'
cargo run --locked -p narrastate-server -- serve --db $database --cases cases --web web/dist
