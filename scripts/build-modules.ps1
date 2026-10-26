param (
    [Parameter(Position=0)]
    [string]$DiskRoot = "build\module-disks",
    [Parameter(Position=1)]
    [UInt64]$CapacityBytes = 67108864
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location (Split-Path -Parent $ScriptDir)

if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Cargo is not installed or not in PATH."
    exit 1
}

$ResolvedDiskRoot = [System.IO.Path]::GetFullPath($DiskRoot)
if (Test-Path $ResolvedDiskRoot) {
    Write-Error "Disk output already exists; choose a new staging directory: $ResolvedDiskRoot"
    exit 1
}

& cargo run --quiet --bin sc16build -- build-bios bios.asm bios.bin
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& cargo run --quiet --bin sc16build -- smoke-modules
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

& cargo run --quiet --bin sc16build -- package-modules $ResolvedDiskRoot $CapacityBytes
exit $LASTEXITCODE
