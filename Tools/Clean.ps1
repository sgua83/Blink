<#
.SYNOPSIS
Remove explicitly selected generated files from Saved or Cargo build output.
.DESCRIPTION
Select paths only after their batch succeeded and the last consumer ended.
Keep unfinished investigations and reusable caches with their original owners.
Cargo metadata owns the build output location; no directory name is inferred.
.PARAMETER Path
Literal workspace-relative generated paths. No automatic age or suffix selection.
Preview and result paths use the same workspace-relative paths.
.EXAMPLE
./Tools/Clean.ps1 -Path Saved/Development/Finished/Output -WhatIf
.EXAMPLE
./Tools/Clean.ps1 -Path target/debug -WhatIf
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)]
    [AllowEmptyCollection()]
    [string[]]$Path,
    [string]$Root = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = 'Stop'
$workspace = (Get-Item -LiteralPath $Root).FullName
$saved = [IO.Path]::GetFullPath((Join-Path $workspace 'Saved'))
$boundary = $saved.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$cargoTarget = $null
$drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($workspace))
$freeBefore = $drive.AvailableFreeSpace

# Generated output boundaries must not be redirected by reparse points.
function Measure-SelectedItem([IO.FileSystemInfo]$Item, [string]$Relative) {
    $pending = [Collections.Generic.Stack[IO.FileSystemInfo]]::new()
    $pending.Push($Item)
    $files = 0L
    $bytes = 0L
    while ($pending.Count -gt 0) {
        $entry = $pending.Pop()
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Reparse point redirects the selected deletion: $relative$($entry.FullName.Substring($Item.FullName.Length))"
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
            if ($null -eq $cargoTarget) {
                Push-Location $workspace
                try {
                    $metadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $workspace 'Cargo.toml')
                    if ($LASTEXITCODE -ne 0) { throw "Cargo metadata failed with exit code $LASTEXITCODE" }
                    $cargoTarget = [IO.Path]::GetFullPath(($metadata | ConvertFrom-Json).target_directory)
                } finally { Pop-Location }
            }
            $cargoBoundary = $cargoTarget.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
            if (-not $absolute.Equals($cargoTarget, [StringComparison]::OrdinalIgnoreCase) -and
                    -not $absolute.StartsWith($cargoBoundary, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Select a generated path below Saved or the Cargo target directory: $relative"
            }
        }
        $native = $absolute
        if ([IO.Path]::DirectorySeparatorChar -eq '\' -and -not $native.StartsWith('\\?\')) {
            $native = if ($native.StartsWith('\\')) { '\\?\UNC\' + $native.Substring(2) } else { '\\?\' + $native }
        }
        $ancestor = [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($native))
        while ($null -ne $ancestor) {
            if (($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Reparse point redirects a parent of the selected path: $relative"
            }
            $ancestor = $ancestor.Parent
        }
        $attributes = [IO.File]::GetAttributes($native)
        $item = if (($attributes -band [IO.FileAttributes]::Directory) -ne 0) {
            [IO.DirectoryInfo]::new($native)
        } else {
            [IO.FileInfo]::new($native)
        }
        if (-not $seen.Add($item.FullName.TrimEnd([IO.Path]::DirectorySeparatorChar))) { continue }
        [pscustomobject]@{ Item = $item; Path = $relative }
    }
)

$selected = @(
    foreach ($entry in $selected) {
        $ancestor = [IO.DirectoryInfo]::new([IO.Path]::GetDirectoryName($entry.Item.FullName))
        while ($null -ne $ancestor -and -not $seen.Contains($ancestor.FullName.TrimEnd([IO.Path]::DirectorySeparatorChar))) {
            $ancestor = $ancestor.Parent
        }
        if ($null -ne $ancestor) { continue }
        $size = Measure-SelectedItem $entry.Item $entry.Path
        [pscustomobject]@{ Item = $entry.Item; Path = $entry.Path; Files = $size.Files; Bytes = $size.Bytes }
    }
)

$rows = @(
    foreach ($entry in $selected) {
        $item = $entry.Item
        $removed = $false
        if ($PSCmdlet.ShouldProcess($entry.Path, 'Remove selected generated intermediates')) {
            if ($item -is [IO.DirectoryInfo]) {
                $item.Delete($true)
            } else {
                $item.Delete()
            }
            $removed = $true
        }
        [pscustomobject]@{
            Path = $entry.Path
            Action = $(if ($removed) { 'Removed' } else { 'Preview' })
            Files = $entry.Files
            LogicalBytes = $entry.Bytes
        }
    }
)
$removedRows = @($rows | Where-Object Action -eq 'Removed')
$freeAfter = $drive.AvailableFreeSpace
[pscustomobject]@{
    Root = '.'
    SelectedFiles = [long]($selected | Measure-Object -Property Files -Sum).Sum
    SelectedLogicalBytes = [long]($selected | Measure-Object -Property Bytes -Sum).Sum
    RemovedFiles = [long]($removedRows | Measure-Object -Property Files -Sum).Sum
    RemovedLogicalBytes = [long]($removedRows | Measure-Object -Property LogicalBytes -Sum).Sum
    FreeBytesBefore = $freeBefore
    FreeBytesAfter = $freeAfter
    FreeBytesDelta = $freeAfter - $freeBefore
    Paths = $rows
}
