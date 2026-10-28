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

$BiosSource = Join-Path $ScriptDir "firmware\bios.asm"
$BiosImage = Join-Path $ScriptDir "firmware\bios.bin"
$RebuildBios = !(Test-Path $BiosImage)
if (!$RebuildBios) {
    $RebuildBios = (Get-Item $BiosSource).LastWriteTimeUtc -gt (Get-Item $BiosImage).LastWriteTimeUtc
}
if ($RebuildBios) {
    Write-Host "Building the SC-16 BIOS firmware."
    & cargo run --quiet --bin sc16build -- build-bios $BiosSource $BiosImage
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

$CargoArgs = @("run", "--quiet", "--bin", "suomicpu16", "--")
if ($Fullscreen) {
    $CargoArgs += "--fullscreen"
}
$TemporaryImage = $null
if (![string]::IsNullOrWhiteSpace($ProgramFile)) {
    $extension = [System.IO.Path]::GetExtension($ProgramFile)
    if (($extension -ieq ".c" -or $extension -ieq ".asm") -and
        (Test-Path -LiteralPath $ProgramFile -PathType Leaf)) {
        $sourcePath = (Resolve-Path -LiteralPath $ProgramFile).Path
        $temporaryName = ".sc16-run-" + [guid]::NewGuid().ToString("N") + ".sc16img"
        $TemporaryImage = Join-Path (Split-Path -Parent $sourcePath) $temporaryName
        & cargo run --quiet --bin sc16build -- $sourcePath $TemporaryImage
        if ($LASTEXITCODE -ne 0) {
            if (Test-Path -LiteralPath $TemporaryImage) {
                Remove-Item -LiteralPath $TemporaryImage -Force
            }
            exit $LASTEXITCODE
        }
        $CargoArgs += $TemporaryImage
    } else {
        $CargoArgs += $ProgramFile
    }
} else {
    Write-Host "Starting SC-16 in BIOS."
}
try {
    & cargo @CargoArgs
    $EmulatorExitCode = $LASTEXITCODE
} finally {
    if ($TemporaryImage -and (Test-Path -LiteralPath $TemporaryImage)) {
        Remove-Item -LiteralPath $TemporaryImage -Force
    }
}
exit $EmulatorExitCode
