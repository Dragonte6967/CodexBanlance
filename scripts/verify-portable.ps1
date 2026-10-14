param([string[]]$Scenarios = @('real', 'success', 'logged-out', 'offline', 'timeout', 'crash'))
$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot -Parent
$qaDirectory = Join-Path $workspace '.qa'
$portableDirectory = Join-Path $qaDirectory '便携 中文 path'
$dataDirectory = Join-Path $env:LOCALAPPDATA 'local.codex.balance'
$settingsPath = Join-Path $dataDirectory 'settings.json'
$reportPath = Join-Path $dataDirectory 'smoke-report.json'
$hadSettings = Test-Path -LiteralPath $settingsPath
$originalSettings = if ($hadSettings) { [IO.File]::ReadAllBytes($settingsPath) } else { $null }
$originalMode = $env:WIDGET_FIXTURE_MODE
New-Item -ItemType Directory -Path $portableDirectory -Force | Out-Null
New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
$exePath = Join-Path $portableDirectory 'CodexBalance.exe'
Copy-Item -LiteralPath (Join-Path $workspace 'release\CodexBalance.exe') -Destination $exePath -Force
$mockPath = Join-Path $qaDirectory 'mock-codex.exe'
& rustc (Join-Path $PSScriptRoot 'mock-codex.rs') -O -C target-feature=+crt-static -o $mockPath
if ($LASTEXITCODE -ne 0) { throw 'Mock server compilation failed' }
if (Get-Process -Name CodexBalance,codex-balance -ErrorAction SilentlyContinue) { throw 'Close the running widget before integration checks.' }
try {
    foreach ($scenario in $Scenarios) {
        $env:WIDGET_FIXTURE_MODE = $scenario
        $config = @{ selected = 'codex' }
        if ($scenario -ne 'real') { $config.cli_path = $mockPath }
        [IO.File]::WriteAllText($settingsPath, ($config | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
        $started = Get-Date
        $widget = Start-Process -FilePath $exePath -WorkingDirectory $portableDirectory -ArgumentList '--smoke-test' -WindowStyle Hidden -PassThru
        Write-Output "Running $scenario integration check (PID $($widget.Id))"
        if (-not $widget.WaitForExit(40000)) { $widget.Kill(); throw "Widget did not exit: $scenario" }
        if (-not (Test-Path -LiteralPath $reportPath) -or (Get-Item -LiteralPath $reportPath).LastWriteTime -lt $started) { throw "Fresh report missing: $scenario" }
        $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($report.decorated -or $report.alwaysOnTop -or -not $report.visible) { throw "Unexpected window flags: $scenario" }
        if ($scenario -eq 'success') {
            if ($report.snapshot.buckets.Count -ne 2 -or $report.snapshot.buckets[0].primary.remaining -ne 75 -or $report.snapshot.error) { throw 'Success fixture mismatch' }
        } elseif ($scenario -eq 'real') {
            if ($report.snapshot.error) { Write-Output "REAL ACCOUNT LIMITATION: $($report.snapshot.error)" }
        } elseif (-not $report.snapshot.error) { throw "Expected error was not reported: $scenario" }
        if (@(Get-ChildItem -LiteralPath $portableDirectory -Force).Count -ne 1) { throw 'Portable directory has extra files' }
        $remainingMock = Get-Process -Name mock-codex -ErrorAction SilentlyContinue
        if ($remainingMock) { throw 'Mock child process survived widget exit' }
        Copy-Item -LiteralPath $reportPath -Destination (Join-Path $qaDirectory "$scenario-report.json") -Force
        Write-Output "PASS $scenario | size=$($report.size -join 'x') | error=$($report.snapshot.error) | portable directory clean"
    }
} finally {
    $env:WIDGET_FIXTURE_MODE = $originalMode
    if ($hadSettings) { [IO.File]::WriteAllBytes($settingsPath, $originalSettings) }
    else { [IO.File]::WriteAllText($settingsPath, '{}', [Text.UTF8Encoding]::new($false)) }
}
