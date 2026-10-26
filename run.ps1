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

Write-Host "Building the SC-16 BIOS firmware."
& cargo run --quiet --bin sc16asm -- bios.asm bios.bin
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$CargoArgs = @("run", "--quiet", "--bin", "suomicpu16", "--")
if ($Fullscreen) {
    $CargoArgs += "--fullscreen"
}
if (![string]::IsNullOrWhiteSpace($ProgramFile)) {
    $CargoArgs += $ProgramFile
} else {
    Write-Host "Starting SC-16 in BIOS."
}
& cargo @CargoArgs
exit $LASTEXITCODE
