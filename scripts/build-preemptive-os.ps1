param (
    [Parameter(Position=0)]
    [string]$DiskPath = "disks\OS000001",
    [Parameter(Position=1)]
    [UInt64]$CapacityBytes = 16777216
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$RepositoryRoot = Split-Path -Parent $ScriptDir
Set-Location $RepositoryRoot

if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Cargo is not installed or not in PATH."
    exit 1
}

$ResolvedDiskPath = [System.IO.Path]::GetFullPath($DiskPath)
if (Test-Path $ResolvedDiskPath) {
    Write-Error "Disk output already exists; choose a fresh path: $ResolvedDiskPath"
    exit 1
}

$StagingRoot = Join-Path $RepositoryRoot ("build\preemptive-os-" + [Guid]::NewGuid().ToString("N"))
$StagingDisk = Join-Path $StagingRoot "OS000001"
$DemoImage = Join-Path $StagingRoot "demo.bin"
New-Item -ItemType Directory -Path $StagingRoot | Out-Null

& cargo run --quiet --bin sc16build -- package-disk examples\preemptive_os_kernel.asm $StagingDisk "SisuOS" $CapacityBytes --kernel
if ($LASTEXITCODE -ne 0) {
    Write-Error "Could not package the preemptive OS kernel."
    exit $LASTEXITCODE
}

& cargo run --quiet --bin sc16build -- examples\preemptive_os_demo.asm $DemoImage
if ($LASTEXITCODE -ne 0) {
    Write-Error "Could not build the preemptive OS demo program."
    exit $LASTEXITCODE
}

Move-Item -LiteralPath $DemoImage -Destination (Join-Path $StagingDisk "demo.bin")
$ManifestPath = Join-Path $StagingDisk "disk.json"
$Manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
$Manifest.protected_files = @("kernel.bin", "demo.bin")
$ManifestJson = ConvertTo-Json -InputObject $Manifest -Depth 8
$Utf8NoBom = New-Object -TypeName System.Text.UTF8Encoding -ArgumentList $false
[System.IO.File]::WriteAllText($ManifestPath, $ManifestJson, $Utf8NoBom)

try {
    $DestinationParent = Split-Path -Parent $ResolvedDiskPath
    if ($DestinationParent -and !(Test-Path $DestinationParent)) {
        New-Item -ItemType Directory -Path $DestinationParent | Out-Null
    }
    Move-Item -LiteralPath $StagingDisk -Destination $ResolvedDiskPath
    Remove-Item -LiteralPath $StagingRoot
} catch {
    Write-Error "Could not install disk at $ResolvedDiskPath. Completed staging disk: $StagingDisk. $($_.Exception.Message)"
    exit 1
}

Write-Output "Built BIOS-bootable preemptive OS disk: $ResolvedDiskPath"
