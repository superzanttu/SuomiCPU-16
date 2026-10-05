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

# With no program path, main.py opens SC-launcher.
if ([string]::IsNullOrWhiteSpace($ProgramFile)) {
    python main.py
} else {
    python main.py $ProgramFile
}
