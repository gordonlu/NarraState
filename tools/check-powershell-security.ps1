$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$excludedDirectories = @('.git', 'node_modules', 'target')
$forbiddenCommands = @(
    'Add-MpPreference',
    'Invoke-Expression',
    'Invoke-RestMethod',
    'Invoke-WebRequest',
    'Register-ScheduledTask',
    'Set-ExecutionPolicy',
    'Set-MpPreference',
    'Start-BitsTransfer',
    'Start-Process',
    'curl',
    'curl.exe',
    'iex',
    'irm',
    'iwr',
    'powershell',
    'powershell.exe',
    'pwsh',
    'reg',
    'reg.exe',
    'schtasks',
    'schtasks.exe',
    'wget'
)
$forbiddenMembers = @(
    'Create',
    'DownloadData',
    'DownloadFile',
    'DownloadString',
    'FromBase64String'
)
$violations = [System.Collections.Generic.List[string]]::new()

function Test-AllowedCommand {
    param(
        [string]$RelativePath,
        [string]$CommandName,
        [System.Management.Automation.Language.CommandAst]$Command
    )

    if ($RelativePath -ne "tools$([System.IO.Path]::DirectorySeparatorChar)smoke-server.ps1") {
        return $false
    }

    if ($CommandName -eq 'Start-Process') {
        return $true
    }

    if ($CommandName -in @('Invoke-RestMethod', 'Invoke-WebRequest')) {
        return $Command.Extent.Text -match 'https?://(127\.0\.0\.1|localhost)(:|/)'
    }

    return $false
}

$scripts = Get-ChildItem -Path $repoRoot -Recurse -File -Include '*.ps1', '*.psm1', '*.psd1' |
    Where-Object {
        $relativePath = [System.IO.Path]::GetRelativePath($repoRoot, $_.FullName)
        $pathSegments = $relativePath -split '[\\/]'
        -not ($excludedDirectories | Where-Object { $pathSegments -contains $_ })
    }

foreach ($script in $scripts) {
    $tokens = $null
    $parseErrors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile(
        $script.FullName,
        [ref]$tokens,
        [ref]$parseErrors
    )
    $relativePath = [System.IO.Path]::GetRelativePath($repoRoot, $script.FullName)

    foreach ($parseError in $parseErrors) {
        $violations.Add("${relativePath}:$($parseError.Extent.StartLineNumber): parse error: $($parseError.Message)")
    }

    $commands = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.CommandAst]
    }, $true)
    foreach ($command in $commands) {
        $commandName = $command.GetCommandName()
        if ($commandName -and
            $forbiddenCommands -contains $commandName -and
            -not (Test-AllowedCommand -RelativePath $relativePath -CommandName $commandName -Command $command)) {
            $violations.Add("${relativePath}:$($command.Extent.StartLineNumber): forbidden command: $commandName")
        }
        if ($command.CommandElements.Extent.Text -match '(?i)(^|\s)-(e|enc|encodedcommand)(\s|$)') {
            $violations.Add("${relativePath}:$($command.Extent.StartLineNumber): encoded PowerShell command is forbidden")
        }
    }

    $memberCalls = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.InvokeMemberExpressionAst]
    }, $true)
    foreach ($memberCall in $memberCalls) {
        $isForbiddenMember = $memberCall.Member -is [System.Management.Automation.Language.StringConstantExpressionAst] -and
            $forbiddenMembers -contains $memberCall.Member.Value
        $isScriptBlockCreation = $memberCall.Member.Value -eq 'Create' -and
            $memberCall.Expression.Extent.Text -match 'ScriptBlock'
        if ($isForbiddenMember -and ($memberCall.Member.Value -ne 'Create' -or $isScriptBlockCreation)) {
            $violations.Add("${relativePath}:$($memberCall.Extent.StartLineNumber): forbidden member call: $($memberCall.Member.Value)")
        }
    }
}

if ($violations.Count -gt 0) {
    $violations | ForEach-Object { [Console]::Error.WriteLine($_) }
    exit 1
}

Write-Output "PowerShell security check passed for $($scripts.Count) tracked scripts."
