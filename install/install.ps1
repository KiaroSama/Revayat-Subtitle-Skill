<#
.SYNOPSIS
Install the skill or plugin. Arguments are forwarded unchanged to install.py.
.EXAMPLE
./install.ps1 --agent codex --scope project --path 'C:\Projects\Anime'
#>
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot 'install.py'
foreach ($candidate in @('python', 'python3', 'py')) {
    $command = Get-Command $candidate -ErrorAction SilentlyContinue
    if ($command) {
        $prefix = if ($candidate -eq 'py') { @('-3') } else { @() }
        & $command.Source @prefix -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)' 2>$null
        if ($LASTEXITCODE -eq 0) {
            & $command.Source @prefix -X utf8 $installer @args
            exit $LASTEXITCODE
        }
    }
}
[Console]::Error.WriteLine('Python 3.10+ is required. Install Python, then run this installer again.')
exit 2
