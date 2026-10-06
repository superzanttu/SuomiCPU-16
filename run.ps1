param (
    [Parameter(Position=0)]
    [string]$ProgramFile,
    [switch]$Fullscreen
)

# Get the directory where the script is located
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

# Check if python is available
if (!(Get-Command python -ErrorAction SilentlyContinue)) {
    Write-Error "Python is not installed or not in PATH."
    exit 1
}

$PythonArgs = @()
if ($Fullscreen) {
    $PythonArgs += "--fullscreen"
}
if (![string]::IsNullOrWhiteSpace($ProgramFile)) {
    $PythonArgs += $ProgramFile
}
python main.py @PythonArgs
