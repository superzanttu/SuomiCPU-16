param(
    [Parameter(Position=0)]
    [string]$Program,
    [switch]$Install
)

$ErrorActionPreference = "Stop"
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    $Command = "build-programs"
    if ($Install) { $Command = "install-programs" }
    $Arguments = @($Command)
    if ($Program) {
        $Arguments[0] = "build-program"
        if ($Install) { $Arguments[0] = "install-program" }
        $Arguments += $Program
    }
    & cargo run --quiet --bin sc16build -- @Arguments
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
