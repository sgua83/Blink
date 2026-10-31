#requires -Version 5.1
param(
    [Parameter(Mandatory = $true)][int]$Change,
    [ValidateSet('patch', 'minor', 'major')][string]$Bump = 'patch',
    [uint64]$UserStage
)
$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$env:P4CHARSET = 'utf8'
$root = Split-Path $PSScriptRoot -Parent
$depot = '//BLanguage/main/'
Push-Location $root
try {
    function Invoke-P4 {
        param([string[]]$Arguments)
        $output = & p4 -ztag -Mj @Arguments 2>&1
        if ($LASTEXITCODE -ne 0) { throw ($output -join "`n") }
        foreach ($line in $output) {
            $record = $line | ConvertFrom-Json
            if ($record.severity -ge 3) { throw $record.data }
            $record
        }
    }
    $opened = @(Invoke-P4 @('opened', '-c', "$Change") | Where-Object { $_.depotFile })
    if ($opened.Count -eq 0) { throw 'Change has no open files on this client.' }
    foreach ($file in $opened) {
        if (-not $file.depotFile.StartsWith($depot, [StringComparison]::Ordinal)) {
            throw 'Submit only the Blink project files in this change.'
        }
    }
    $status = Invoke-P4 @('fstat', 'VERSION')
    if ($status.action -and $status.change -ne "$Change") {
        throw 'VERSION is opened in another change on this client.'
    }
    $previous = ((Invoke-P4 @('print', '-q', ($depot + 'VERSION#head')) | ForEach-Object { $_.data }) -join '').Trim()
    if ($previous -notmatch '^([0-9]+)\.([0-9]+)\.([0-9]+)\.([0-9]+)$') {
        throw 'Submitted VERSION must contain user.major.minor.patch.'
    }
    $stage = if ($PSBoundParameters.ContainsKey('UserStage')) { $UserStage } else { [uint64]$Matches[1] }
    $major = [long]$Matches[2]; $minor = [long]$Matches[3]; $patch = [long]$Matches[4]
    $next = switch ($Bump) {
        'major' { '{0}.{1}.0.0' -f $stage, ($major + 1) }
        'minor' { '{0}.{1}.{2}.0' -f $stage, $major, ($minor + 1) }
        'patch' { '{0}.{1}.{2}.{3}' -f $stage, $major, $minor, ($patch + 1) }
    }
    Invoke-P4 @('edit', '-c', "$Change", 'VERSION') | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $root 'VERSION'), "$next`n", $utf8)
    Invoke-P4 @('submit', '-c', "$Change") | ConvertTo-Json -Compress
    Write-Output "BLINK_VERSION $next"
} finally { Pop-Location }
