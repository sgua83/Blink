#requires -Version 7.2
param([string]$P4Client = 'BLanguage_Gua')

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$depot = '//BLanguage/main/'
$utf8 = [Text.UTF8Encoding]::new($false)

function Invoke-CommandText {
    param([string]$File, [string[]]$Arguments, [byte[]]$InputBytes)
    $start = [Diagnostics.ProcessStartInfo]::new($File)
    $start.WorkingDirectory = $root
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.RedirectStandardInput = $true
    $start.StandardOutputEncoding = $utf8
    $start.StandardErrorEncoding = $utf8
    $start.Environment['P4CHARSET'] = 'utf8'
    $start.Environment['GIT_TERMINAL_PROMPT'] = '0'
    $start.Environment['GCM_INTERACTIVE'] = 'Never'
    $start.Environment['GIT_OPTIONAL_LOCKS'] = '0'
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($start)
    $outputTask = $process.StandardOutput.ReadToEndAsync()
    $errorTask = $process.StandardError.ReadToEndAsync()
    if ($InputBytes) { $process.StandardInput.BaseStream.Write($InputBytes) }
    $process.StandardInput.Close()
    $process.WaitForExit()
    $output = $outputTask.GetAwaiter().GetResult()
    $errorOutput = $errorTask.GetAwaiter().GetResult()
    $code = $process.ExitCode
    $process.Dispose()
    if ($code -ne 0) { throw "$File exited $code`: $errorOutput$output" }
    $output.TrimEnd()
}

function Git {
    param([string[]]$Arguments, [byte[]]$InputBytes)
    Invoke-CommandText git $Arguments $InputBytes
}

function P4Records {
    param([string[]]$Arguments)
    $text = Invoke-CommandText p4 (@('-c', $P4Client, '-ztag', '-Mj') + $Arguments)
    foreach ($line in $text -split "`n") {
        if (-not $line.Trim()) { continue }
        $record = $line | ConvertFrom-Json
        if ($record.code -eq 'error') { throw $record.data }
        $record
    }
}

$remote = Git @('remote', 'get-url', 'origin')
if ($remote -ne 'https://github.com/sgua83/Blink.git') {
    throw 'origin must identify the authorized sgua83/Blink repository.'
}
$branch = Git @('symbolic-ref', '--short', 'HEAD')
if ($branch -ne 'main') { throw 'The publishing branch must be main.' }
$head = Git @('for-each-ref', '--format=%(objectname)', 'refs/heads/main')
Git @('fetch', 'origin') | Out-Null
$remoteHead = Git @('ls-remote', '--heads', 'origin', 'main')
if ($remoteHead -and ($remoteHead -split '\s+')[0] -ne $head) {
    throw 'GitHub main differs from local main; publication requires resolving Git history.'
}
$message = if ($head) { Git @('log', '-1', '--format=%B', $head) } else { '' }
$lastChange = if ($message -match '(?m)^P4-Change: (\d+)\s*$') { [int]$Matches[1] } else { 0 }
$latest = @(P4Records @('changes', '-m', '1', '-s', 'submitted', "$depot..."))[0]
$latestChange = [int]$latest.change
if ($lastChange -eq $latestChange -and $remoteHead) {
    Write-Output "UNCHANGED P4 CL $lastChange"
    exit 0
}
$changes = if ($lastChange -eq 0) { @($latest) } else {
    $lower = $lastChange + 1
    @(P4Records @('changes', '-s', 'submitted', "$depot...@$lower,$latestChange") | Sort-Object { [int]$_.change })
}
$gitDirectory = Git @('rev-parse', '--absolute-git-dir')
$index = Join-Path $gitDirectory 'index'
$indexLock = Join-Path $gitDirectory 'index.lock'
# Use Git's index ownership boundary. P4 operations do not use this lock.
$lock = [IO.File]::Open($indexLock, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
$runDirectory = Join-Path $root ('Saved/GitPublish/' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($runDirectory) | Out-Null
$savedIndex = $env:GIT_INDEX_FILE
$parent = $head
try {
    Git @('diff', '--cached', '--quiet') | Out-Null
    $env:GIT_INDEX_FILE = Join-Path $runDirectory 'index'
    if ($head) { Git @('read-tree', $head) | Out-Null } else { Git @('read-tree', '--empty') | Out-Null }
    foreach ($change in $changes) {
        $number = [int]$change.change
        $entries = if ($lastChange -eq 0) {
            @(P4Records @('files', "$depot...@$number"))
        } else {
            $description = @(P4Records @('describe', '-s', "$number"))[0]
            for ($i = 0; $description.PSObject.Properties["depotFile$i"]; $i++) {
                [pscustomobject]@{
                    depotFile = $description."depotFile$i"
                    rev = $description."rev$i"
                    action = $description."action$i"
                    type = $description."type$i"
                }
            }
        }
        $lines = [Text.StringBuilder]::new()
        $fileNumber = 0
        foreach ($entry in $entries) {
            if (-not $entry.depotFile.StartsWith($depot, [StringComparison]::Ordinal)) { continue }
            $relative = $entry.depotFile.Substring($depot.Length)
            if ($entry.action -in @('delete', 'move/delete')) {
                if ($lastChange -ne 0) { [void]$lines.Append("0 $([string]::new('0', 40))`t$relative`0") }
                continue
            }
            $fileNumber++
            $blob = Join-Path $runDirectory ("$number-$fileNumber.bin")
            $fileList = Join-Path $runDirectory 'print-files.txt'
            [IO.File]::WriteAllText($fileList, "$($entry.depotFile)#$($entry.rev)`n", $utf8)
            Invoke-CommandText p4 @('-c', $P4Client, '-x', $fileList, 'print', '-q', '-o', $blob) | Out-Null
            $object = Git @('hash-object', '-w', $blob)
            $mode = if ($entry.type -eq 'symlink') { '120000' } elseif ($entry.type -match '\+.*x') { '100755' } else { '100644' }
            [void]$lines.Append("$mode $object`t$relative`0")
        }
        Git @('update-index', '-z', '--index-info') ($utf8.GetBytes($lines.ToString())) | Out-Null
        $tree = Git @('write-tree')
        $description = Invoke-CommandText p4 @('-c', $P4Client, '-ztag', '-Mj', 'describe', '-s', "$number") | ConvertFrom-Json
        $subject = ($description.desc -split "`n")[0]
        $commitMessage = "$subject`n`nP4-Change: $number`nP4-Depot: $depot`n"
        $commitArguments = @('commit-tree', $tree)
        if ($parent) { $commitArguments += @('-p', $parent) }
        $parent = Git $commitArguments ($utf8.GetBytes($commitMessage))
        Write-Output "PREPARED P4 CL $number -> $parent"
    }
    Git @('push', 'origin', "${parent}:refs/heads/main") | Out-Null
    $published = Git @('ls-remote', '--heads', 'origin', 'main')
    if (($published -split '\s+')[0] -ne $parent) { throw 'GitHub did not confirm the published commit.' }
    $previous = if ($head) { $head } else { [string]::new('0', 40) }
    Git @('update-ref', 'refs/heads/main', $parent, $previous) | Out-Null
    $indexBytes = [IO.File]::ReadAllBytes($env:GIT_INDEX_FILE)
    $lock.Write($indexBytes)
    $lock.Dispose()
    $lock = $null
    [IO.File]::Move($indexLock, $index, $true)
    Write-Output "PUBLISHED P4 CL $latestChange -> $parent"
} finally {
    $env:GIT_INDEX_FILE = $savedIndex
    if ($lock) { $lock.Dispose(); Remove-Item -LiteralPath $indexLock }
    foreach ($file in Get-ChildItem -LiteralPath $runDirectory -File) {
        if ($file.IsReadOnly) { $file.IsReadOnly = $false }
        Remove-Item -LiteralPath $file.FullName
    }
    Remove-Item -LiteralPath $runDirectory
}
