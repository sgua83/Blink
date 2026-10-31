#requires -Version 7.2
param(
    [string]$P4Client = 'BLanguage_Gua',
    [string]$MirrorDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$depot = '//BLanguage/main/'
$p4Only = @('Docs/Issues.md', 'Docs/FixedIssues.md')
$utf8 = [Text.UTF8Encoding]::new($false)
if (-not $MirrorDirectory) { $MirrorDirectory = Join-Path $root 'Saved/GitPublish/Mirror.git' }
$mirror = [IO.Path]::GetFullPath($MirrorDirectory)

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
    Invoke-CommandText git (@('--git-dir', $mirror) + $Arguments) $InputBytes
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

[IO.Directory]::CreateDirectory($mirror) | Out-Null
# The OS handle owns this publisher's repository for this invocation; no stale lock state.
$lock = [IO.FileStream]::new((Join-Path $mirror 'publish.lock'), [IO.FileMode]::OpenOrCreate,
    [IO.FileAccess]::ReadWrite, [IO.FileShare]::None, 4096, [IO.FileOptions]::DeleteOnClose)
$runDirectory = Join-Path $mirror ('run-' + [Guid]::NewGuid().ToString('N'))
$savedIndex = $env:GIT_INDEX_FILE
$transcript = $false
try {
    Start-Transcript -Path (Join-Path $mirror 'publish.log') -Append | Out-Null
    $transcript = $true
    if (-not (Test-Path -LiteralPath (Join-Path $mirror 'config'))) {
        Invoke-CommandText git @('init', '--bare', $mirror) | Out-Null
        Git @('remote', 'add', 'origin', 'https://github.com/sgua83/Blink.git') | Out-Null
    }
    $remote = Git @('config', '--get', 'remote.origin.url')
    if ($remote -ne 'https://github.com/sgua83/Blink.git') {
        throw 'origin must identify the authorized sgua83/Blink repository.'
    }
    Git @('fetch', '--no-tags', 'origin', 'refs/heads/main:refs/remotes/origin/main') | Out-Null
    $head = Git @('rev-parse', 'refs/remotes/origin/main')
    $message = Git @('log', '-1', '--format=%B', $head)
    if ($message -notmatch '(?m)^P4-Change: (\d+)\s*$') {
        throw 'The published main commit has no P4-Change watermark.'
    }
    $lastChange = [int]$Matches[1]
    if ($message -notmatch '(?m)^P4-Depot: //BLanguage/main/\s*$') {
        throw 'The published watermark belongs to another P4 depot.'
    }
    $latest = @(P4Records @('changes', '-m', '1', '-s', 'submitted', "$depot..."))[0]
    $latestChange = [int]$latest.change
    if ($lastChange -gt $latestChange) { throw 'The published watermark is newer than the P4 source.' }
    if ($lastChange -eq $latestChange) {
        Write-Output "UNCHANGED P4 CL $lastChange"
        exit 0
    }
    $lower = $lastChange + 1
    $changes = @(P4Records @('changes', '-s', 'submitted', "$depot...@$lower,$latestChange") |
        Sort-Object { [int]$_.change })
    [IO.Directory]::CreateDirectory($runDirectory) | Out-Null
    $env:GIT_INDEX_FILE = Join-Path $runDirectory 'index'
    Git @('read-tree', $head) | Out-Null
    $parent = $head
    foreach ($change in $changes) {
        $number = [int]$change.change
        $description = @(P4Records @('describe', '-s', "$number"))[0]
        $entries = for ($i = 0; $description.PSObject.Properties["depotFile$i"]; $i++) {
            [pscustomobject]@{
                depotFile = $description."depotFile$i"
                rev = $description."rev$i"
                action = $description."action$i"
                type = $description."type$i"
            }
        }
        $lines = [Text.StringBuilder]::new()
        # Both ledgers are P4-only, including entries in a previous Git tree.
        foreach ($ledger in $p4Only) {
            [void]$lines.Append("0 $([string]::new('0', 40))`t$ledger`0")
        }
        $fileNumber = 0
        foreach ($entry in $entries) {
            if (-not $entry.depotFile.StartsWith($depot, [StringComparison]::Ordinal)) { continue }
            $relative = $entry.depotFile.Substring($depot.Length)
            if ($relative -cin $p4Only) { continue }
            if ($entry.action -in @('delete', 'move/delete')) {
                [void]$lines.Append("0 $([string]::new('0', 40))`t$relative`0")
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
        $subject = ($description.desc -split "`n")[0]
        $commitMessage = "$subject`n`nP4-Change: $number`nP4-Depot: $depot`nP4-User: $($description.user)`n"
        $commitArguments = @('commit-tree', $tree)
        if ($parent) { $commitArguments += @('-p', $parent) }
        $parent = Git $commitArguments ($utf8.GetBytes($commitMessage))
        Write-Output "PREPARED P4 CL $number -> $parent"
    }
    Git @('push', 'origin', "${parent}:refs/heads/main") | Out-Null
    $published = Git @('ls-remote', '--heads', 'origin', 'main')
    if (($published -split '\s+')[0] -ne $parent) { throw 'GitHub did not confirm the published commit.' }
    Git @('update-ref', 'refs/heads/main', $parent) | Out-Null
    Write-Output "PUBLISHED P4 CL $latestChange -> $parent"
} catch {
    Write-Output "FAILED: $($_.Exception.Message)"
    throw
} finally {
    $env:GIT_INDEX_FILE = $savedIndex
    if (Test-Path -LiteralPath $runDirectory) {
        foreach ($file in Get-ChildItem -LiteralPath $runDirectory -File) {
            if ($file.IsReadOnly) { $file.IsReadOnly = $false }
            Remove-Item -LiteralPath $file.FullName
        }
        Remove-Item -LiteralPath $runDirectory
    }
    if ($transcript) { Stop-Transcript | Out-Null }
    $lock.Dispose()
}
