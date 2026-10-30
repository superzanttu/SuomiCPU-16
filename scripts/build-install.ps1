param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^[a-z0-9][a-z0-9-]*$')]
    [string]$Program,
    [switch]$Start
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$ManifestPath = Join-Path (Join-Path $RepositoryRoot "programs\$Program") "program.json"
if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
    throw "No program manifest found for '$Program'."
}

Push-Location $RepositoryRoot
try {
    & cargo run --quiet --locked --bin sc16build -- install-program $Program
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    if ($Start) {
        $Manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
        $InstallDisk = $Manifest.install_disk
        if (-not $InstallDisk) { $InstallDisk = $Program }
        & cargo run --quiet --locked --bin suomicpu16 -- --disk $InstallDisk
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}
