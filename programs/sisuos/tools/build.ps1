param([switch]$Install)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
Push-Location $RepositoryRoot
try {
    $Command = "build-program"
    if ($Install) { $Command = "install-program" }
    & cargo run --quiet --bin sc16build -- $Command sisuos
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
