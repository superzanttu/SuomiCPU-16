param (
    [Parameter(Position=0)]
    [string]$ProgramFile,
    [switch]$Fullscreen
)

# Get the directory where the script is located
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

# Check if Cargo is available
if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Cargo is not installed or not in PATH."
    exit 1
}

$RustArgs = @()
if ($Fullscreen) {
    $RustArgs += "--fullscreen"
}
if (![string]::IsNullOrWhiteSpace($ProgramFile)) {
    $RustArgs += $ProgramFile
}
cargo run --quiet --bin suomicpu16 -- @RustArgs
exit $LASTEXITCODE
