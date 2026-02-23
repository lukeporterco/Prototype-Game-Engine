param(
    [Parameter(Mandatory = $false)]
    [ValidateSet("list", "run-one")]
    [string]$Mode,

    [Parameter(Mandatory = $false)]
    [ValidateSet("engine", "game", "thruport_cli")]
    [string]$Package,

    [Parameter(Mandatory = $false)]
    [string]$Pattern,

    [Parameter(Mandatory = $false)]
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Show-Usage {
    @"
test-helper.ps1

Usage:
  pwsh -File scripts/test-helper.ps1 -Mode list -Package <engine|game|thruport_cli> [-Pattern <regex>]
  pwsh -File scripts/test-helper.ps1 -Mode run-one -Package <engine|game|thruport_cli> -Pattern <regex>

Examples:
  pwsh -File scripts/test-helper.ps1 -Mode list -Package engine
  pwsh -File scripts/test-helper.ps1 -Mode list -Package game -Pattern "scenario_setup"
  pwsh -File scripts/test-helper.ps1 -Mode run-one -Package game -Pattern "scenario_setup_combat_chaser_is_idempotent"
"@ | Write-Output
}

function Fail {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Message
    )
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Get-CanonicalTestNames {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Pkg
    )

    $lines = & cargo test -p $Pkg -- --list
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    $tests = @()
    foreach ($line in $lines) {
        if ($line -match '^(?<name>.+): test$') {
            $tests += $matches['name']
        }
    }
    return $tests
}

if ($Help) {
    Show-Usage
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Mode) -or [string]::IsNullOrWhiteSpace($Package)) {
    Fail "missing required arguments: -Mode and -Package are required. use -Help for usage."
}

if ($Mode -eq "run-one" -and [string]::IsNullOrWhiteSpace($Pattern)) {
    Fail "missing required argument: -Pattern is required for -Mode run-one."
}

$allTests = @(Get-CanonicalTestNames -Pkg $Package)

$matches = @(if ([string]::IsNullOrWhiteSpace($Pattern)) {
    $allTests
} else {
    $allTests | Where-Object { $_ -match $Pattern }
})

if ($Mode -eq "list") {
    foreach ($name in $matches) {
        Write-Output $name
    }
    exit 0
}

if ($matches.Count -eq 0) {
    Fail "no tests matched regex '$Pattern' in package '$Package'."
}

if ($matches.Count -gt 1) {
    [Console]::Error.WriteLine("pattern '$Pattern' matched $($matches.Count) tests in package '$Package'. refine the pattern.")
    Write-Output "Matched tests:"
    foreach ($name in $matches) {
        Write-Output "  $name"
    }
    exit 1
}

$exactName = $matches[0]
Write-Output "Matched 1 test: $exactName"
Write-Output "Running exact: cargo test -p $Package $exactName -- --exact"
& cargo test -p $Package $exactName -- --exact
exit $LASTEXITCODE
