#requires -Version 7.2
param(
    [string]$P4Client = 'BLanguage_Gua',
    [ValidateSet('Blink.P4GitMirror', 'Blink.WeeklyRelease')]
    [string[]]$TaskName = @('Blink.P4GitMirror', 'Blink.WeeklyRelease')
)
$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$env:P4CHARSET = 'utf8'
$infoLines = & p4 -c $P4Client -ztag -Mj info
if ($LASTEXITCODE -ne 0) { throw 'Unable to resolve the central P4 workspace.' }
$info = @($infoLines | ForEach-Object { $_ | ConvertFrom-Json })[0]
$root = $info.clientRoot
$publisher = Join-Path $root 'Tools/PublishBooksWiki.py'
if (-not (Test-Path -LiteralPath $publisher)) { throw 'Sync the submitted Wiki publisher into the central workspace first.' }
$python = (Get-Command python -CommandType Application | Select-Object -First 1).Source
$p4 = (Get-Command p4 -CommandType Application | Select-Object -First 1).Source
$escapedPython = $python.Replace("'", "''")
$escapedP4 = $p4.Replace("'", "''")
$escapedClient = $P4Client.Replace("'", "''")
$command = @"
`$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new(`$false)
`$env:PYTHONIOENCODING = 'utf-8'
`$env:P4CHARSET = 'utf8'
`$env:P4PORT = 'P4-Server:1666'
`$env:P4USER = 'Gua'
`$lines = & '$escapedP4' -c '$escapedClient' -ztag -Mj info
if (`$LASTEXITCODE -ne 0) { throw 'Unable to resolve the central P4 workspace.' }
`$info = @(`$lines | ForEach-Object { `$_ | ConvertFrom-Json })[0]
& '$escapedPython' (Join-Path `$info.clientRoot 'Tools/PublishBooksWiki.py')
exit `$LASTEXITCODE
"@
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
foreach ($name in $TaskName) {
    $task = Get-ScheduledTask -TaskName $name -TaskPath '\'
    $original = @($task.Actions | Where-Object { $_.Id -ne 'BlinkBooksWiki' })
    if ($original.Count -ne 1) { throw "$name must retain exactly its existing mirror/release action." }
    $action = New-ScheduledTaskAction -Execute $original[0].Execute -Argument "-NoLogo -NoProfile -NonInteractive -EncodedCommand $encoded"
    $action.Id = 'BlinkBooksWiki'
    Set-ScheduledTask -TaskName $name -TaskPath '\' -Action @($original[0], $action) -ErrorAction Stop | Out-Null
    $adopted = Get-ScheduledTask -TaskName $name -TaskPath '\'
    if ($adopted.Actions.Count -ne 2 -or $adopted.Actions[1].Id -ne 'BlinkBooksWiki' -or $adopted.Actions[1].Arguments -ne $action.Arguments) {
        throw "$name did not adopt the Wiki action. Inspect its actual definition."
    }
    Write-Output "BOOKS_WIKI_INSTALLED $name (existing schedule, principal and first action retained)"
}
