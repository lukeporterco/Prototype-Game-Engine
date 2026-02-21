$ErrorActionPreference='Stop'
$scriptPath = Join-Path $PSScriptRoot 'run_minset_telemetry.ps1'
if(-not (Test-Path $scriptPath)){ throw "missing script: $scriptPath" }
& powershell -ExecutionPolicy Bypass -File $scriptPath