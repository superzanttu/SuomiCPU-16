param (
    [Parameter(Position=0)]
    [string]$ProgramFile
)

# Get the directory where the script is located
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

# Check if python is available
if (!(Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Error "Python is not installed or not in PATH."
    exit 1
}

if ([string]::IsNullOrWhiteSpace($ProgramFile)) {
    Write-Host "Usage: .\run.ps1 <program_file>" -ForegroundColor Yellow
    Write-Host "Example: .\run.ps1 .\examples\asteroids.c" -ForegroundColor Yellow
    exit 0
}

# Run the emulator via main.py
python main.py $ProgramFile
