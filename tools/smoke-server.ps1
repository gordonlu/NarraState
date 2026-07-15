param(
    [int]$Port = 3217
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$binaryName = if ($IsWindows) { 'narrastate-server.exe' } else { 'narrastate-server' }
$binary = Join-Path $repoRoot "target/debug/$binaryName"
if (-not (Test-Path $binary)) {
    throw "Server binary not found at $binary. Run cargo build -p narrastate-server first."
}
if (-not (Test-Path 'web/dist/index.html')) {
    throw 'web/dist is missing. Run npm --prefix web run build first.'
}

$database = Join-Path ([System.IO.Path]::GetTempPath()) "narrastate-smoke-$([guid]::NewGuid()).db"
$stdout = Join-Path ([System.IO.Path]::GetTempPath()) "narrastate-smoke-$([guid]::NewGuid()).out.log"
$stderr = Join-Path ([System.IO.Path]::GetTempPath()) "narrastate-smoke-$([guid]::NewGuid()).err.log"
$arguments = @('serve', '--port', "$Port", '--db', $database, '--cases', 'cases', '--web', 'web/dist')
$env:NARRASTATE_HOST = '127.0.0.1'
$process = Start-Process -FilePath $binary -ArgumentList $arguments -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr

try {
    $healthy = $false
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        if ($process.HasExited) {
            throw "Server exited early with code $($process.ExitCode): $(Get-Content $stderr -Raw)"
        }
        try {
            $response = Invoke-RestMethod "http://127.0.0.1:$Port/api/v1/health" -TimeoutSec 1
            if ($response.status -eq 'ok') {
                $healthy = $true
                break
            }
        } catch {
            Start-Sleep -Milliseconds 250
        }
    }
    if (-not $healthy) {
        throw "Health check timed out: $(Get-Content $stderr -Raw)"
    }
    $homeResponse = Invoke-WebRequest "http://127.0.0.1:$Port/" -TimeoutSec 3
    if ($homeResponse.StatusCode -ne 200 -or $homeResponse.Content -notmatch '<div id="app"></div>') {
        throw 'Static web shell did not load correctly.'
    }
    Write-Host "NarraState startup smoke passed on $($PSVersionTable.OS)"
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    Remove-Item $database, "$database-shm", "$database-wal", $stdout, $stderr -Force -ErrorAction SilentlyContinue
}
