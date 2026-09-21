<#
.SYNOPSIS
Install the skill or plugin with UTF-8 bootstrap diagnostics.
#>
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot 'install.py'
$script:writer = $null
$script:threshold = 20
$levels = @{ DEBUG = 10; INFO = 20; WARNING = 30; ERROR = 40; CRITICAL = 50 }
function Write-BootstrapLog([string]$level, [string]$message) {
    if ($levels[$level] -lt $script:threshold) { return }
    $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
    $line = "[$stamp UTC] [$level] [install-bootstrap] $message"
    if ($script:writer) {
        try { $script:writer.WriteLine($line) }
        catch {
            try { $script:writer.Dispose() } catch {}
            $script:writer = $null
            [Console]::Error.WriteLine("[$stamp UTC] [WARNING] [install-bootstrap] File logging failed; using stderr.")
        }
    }
    if (-not $script:writer -or $levels[$level] -ge 30) { [Console]::Error.WriteLine($line) }
}
try {
    $level = if ($env:REVAYAT_LOG_LEVEL) { $env:REVAYAT_LOG_LEVEL.Trim().ToUpperInvariant() } else { 'INFO' }
    if (-not $levels.ContainsKey($level)) {
        Write-BootstrapLog 'ERROR' 'Invalid REVAYAT_LOG_LEVEL; use DEBUG, INFO, WARNING, ERROR or CRITICAL.'
        exit 2
    }
    $script:threshold = $levels[$level]
    $directory = if ($env:REVAYAT_LOG_DIR) { $env:REVAYAT_LOG_DIR } else { Join-Path $PSScriptRoot '../skills/revayat-subtitle/logs' }
    try {
        [IO.Directory]::CreateDirectory($directory) | Out-Null
        $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd_HH-mm-ss')
        for ($number = 0; $number -lt 1000; $number++) {
            $suffix = if ($number) { "_$number" } else { '' }
            $path = Join-Path $directory "install-bootstrap_${stamp}_UTC$suffix.log"
            try {
                $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
                $script:writer = New-Object IO.StreamWriter($stream, (New-Object Text.UTF8Encoding($false)))
                $script:writer.AutoFlush = $true
                break
            } catch [IO.IOException] {
                if (-not [IO.File]::Exists($path)) { throw }
            }
        }
        if (-not $script:writer) { throw 'Log collision limit' }
    } catch {
        $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
        [Console]::Error.WriteLine("[$stamp UTC] [WARNING] [install-bootstrap] File logging unavailable; using stderr.")
    }
    Write-BootstrapLog 'INFO' 'Checking Python prerequisite.'
    foreach ($candidate in @('python', 'python3', 'py')) {
        $command = Get-Command $candidate -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($command) {
            $prefix = if ($candidate -eq 'py') { @('-3') } else { @() }
            & $command.Source @prefix -B -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)' 2>$null
            if ($LASTEXITCODE -eq 0) {
                Write-BootstrapLog 'DEBUG' 'Compatible interpreter found; forwarding arguments without logging their values.'
                & $command.Source @prefix -B -X utf8 $installer @args
                $result = $LASTEXITCODE
                $severity = if ($result -eq 0) { 'INFO' } else { 'ERROR' }
                Write-BootstrapLog $severity "Installer completed exit=$result."
                exit $result
            }
        }
    }
    Write-BootstrapLog 'ERROR' 'Python 3.10+ is required. Install Python, then run this installer again.'
    exit 2
} catch {
    Write-BootstrapLog 'ERROR' 'Installer bootstrap failed; check the interpreter and filesystem permissions.'
    exit 2
} finally {
    if ($script:writer) {
        try { $script:writer.Dispose() }
        catch {
            $stamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
            [Console]::Error.WriteLine("[$stamp UTC] [WARNING] [install-bootstrap] Log close failed.")
        }
    }
}
