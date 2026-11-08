<#
.SYNOPSIS
Remove explicitly selected generated files below this workspace's Saved directory.
.DESCRIPTION
Select paths only after their batch succeeded and the last consumer ended.
Keep unfinished investigations and reusable caches with their original owners.
Cargo build directories are cleaned by Cargo, outside this command.
.PARAMETER Path
Literal workspace-relative paths below Saved. No automatic age or suffix selection.
.EXAMPLE
./Tools/Clean.ps1 -Path Saved/Development/Finished/Output -WhatIf
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path,
    [string]$Root = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
$workspace = (Get-Item -LiteralPath $Root).FullName
$saved = [IO.Path]::GetFullPath((Join-Path $workspace 'Saved'))
$boundary = $saved.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($workspace))
$freeBefore = $drive.AvailableFreeSpace

# The Saved boundary authorizes deletion; reparse points must not redirect it.
function Measure-SelectedItem([IO.FileSystemInfo]$Item) {
    $pending = [Collections.Generic.Stack[IO.FileSystemInfo]]::new()
    $pending.Push($Item)
    $files = 0L
    $bytes = 0L
    while ($pending.Count -gt 0) {
        $entry = $pending.Pop()
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Reparse point redirects the selected deletion: $($entry.FullName)"
        }
        if ($entry -is [IO.DirectoryInfo]) {
            foreach ($child in $entry.EnumerateFileSystemInfos()) { $pending.Push($child) }
        } else {
            $files++
            $bytes += $entry.Length
        }
    }
    [pscustomobject]@{ Files = $files; Bytes = $bytes }
}

$seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$selected = @(
    foreach ($relative in $Path) {
        if ([IO.Path]::IsPathRooted($relative)) {
            throw "Path must be relative to the workspace: $relative"
        }
        $absolute = [IO.Path]::GetFullPath((Join-Path $workspace $relative))
        if (-not $absolute.StartsWith($boundary, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Select a specific path below Saved: $relative"
        }
        if (-not $seen.Add($absolute)) { continue }
        $ancestor = [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($absolute))
        while ($null -ne $ancestor) {
            if (($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Reparse point redirects the selected deletion: $($ancestor.FullName)"
            }
            $ancestor = $ancestor.Parent
        }
        $item = Get-Item -LiteralPath $absolute
        $size = Measure-SelectedItem $item
        [pscustomobject]@{ Item = $item; Files = $size.Files; Bytes = $size.Bytes }
    }
)

$rows = @(
    foreach ($entry in $selected) {
        $item = $entry.Item
        $removed = $false
        if ($PSCmdlet.ShouldProcess($item.FullName, 'Remove selected generated intermediates')) {
            if ($item -is [IO.DirectoryInfo]) {
                Remove-Item -LiteralPath $item.FullName -Recurse
            } else {
                Remove-Item -LiteralPath $item.FullName
            }
            $removed = $true
        }
        [pscustomobject]@{
            Path = $item.FullName
            Action = $(if ($removed) { 'Removed' } else { 'Preview' })
            Files = $entry.Files
            LogicalBytes = $entry.Bytes
        }
    }
)
$removedRows = @($rows | Where-Object Action -eq 'Removed')
$freeAfter = $drive.AvailableFreeSpace
[pscustomobject]@{
    Root = $workspace
    SelectedFiles = ($selected | Measure-Object -Property Files -Sum).Sum
    SelectedLogicalBytes = ($selected | Measure-Object -Property Bytes -Sum).Sum
    RemovedFiles = [long]($removedRows | Measure-Object -Property Files -Sum).Sum
    RemovedLogicalBytes = [long]($removedRows | Measure-Object -Property LogicalBytes -Sum).Sum
    FreeBytesBefore = $freeBefore
    FreeBytesAfter = $freeAfter
    FreeBytesDelta = $freeAfter - $freeBefore
    Paths = $rows
}
