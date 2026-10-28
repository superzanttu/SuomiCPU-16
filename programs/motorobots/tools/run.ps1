$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot))
Push-Location $RepositoryRoot
try {
    & .\scripts\build-programs.ps1 motorobots -Install
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & .\run.ps1 -ProgramFile .\disks\motorobots
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
