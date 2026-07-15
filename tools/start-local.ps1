$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

npm --prefix web ci
npm --prefix web run build
$database = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { 'data/narrastate.db' }
cargo run --locked -p narrastate-server -- serve --db $database --cases cases --web web/dist
