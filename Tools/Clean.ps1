<#
.SYNOPSIS
Remove intermediate build output and temporary compiled files.
.DESCRIPTION
Without Path, clear Cargo build output and temporary compiled files under Saved.
This complete cleanup includes reusable caches and development binaries.
Finish active consumers first. Keep source, logs, installed tools and dist assets.
Pass Path to select literal generated files instead of the complete cleanup.
Paths are always relative to the project containing this script.
.PARAMETER Path
Literal workspace-relative generated paths. No automatic age or suffix selection.
Preview and result paths use the same workspace-relative paths.
.EXAMPLE
./Tools/Clean.ps1 -WhatIf
.EXAMPLE
./Tools/Clean.ps1
.EXAMPLE
./Tools/Clean.ps1 -Path Saved/Development/Finished/Output -WhatIf
.EXAMPLE
./Tools/Clean.ps1 -Path target/debug -WhatIf
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [AllowEmptyCollection()]
    [string[]]$Path
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$workspaceBoundary = $workspace.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$saved = [IO.Path]::GetFullPath((Join-Path $workspace 'Saved'))
$boundary = $saved.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$cargoTarget = $null
$drive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($workspace))
$freeBefore = $drive.AvailableFreeSpace

if (-not $PSBoundParameters.ContainsKey('Path')) {
    Push-Location $workspace
    try {
        $metadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $workspace 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw "Cargo metadata failed with exit code $LASTEXITCODE" }
        $cargoTarget = [IO.Path]::GetFullPath(($metadata | ConvertFrom-Json).target_directory)
    } finally { Pop-Location }
    if (-not $cargoTarget.StartsWith($workspaceBoundary, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Cargo build output is outside this workspace.'
    }
    $nativeWorkspace = $workspace
    if ([IO.Path]::DirectorySeparatorChar -eq '\' -and -not $nativeWorkspace.StartsWith('\\?\')) {
        $nativeWorkspace = if ($nativeWorkspace.StartsWith('\\')) { '\\?\UNC\' + $nativeWorkspace.Substring(2) } else { '\\?\' + $nativeWorkspace }
    }
    $nativeBoundary = $nativeWorkspace.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $discovered = [Collections.Generic.List[string]]::new()
    $buildOutputs = [Collections.Generic.List[IO.DirectoryInfo]]::new()
    $nativeTarget = $nativeBoundary + $cargoTarget.Substring($workspaceBoundary.Length)
    if ([IO.Directory]::Exists($nativeTarget)) { $buildOutputs.Add([IO.DirectoryInfo]::new($nativeTarget)) }
    $compiledExtensions = [Collections.Generic.HashSet[string]]::new([string[]]@(
        '.obj', '.o', '.lib', '.a', '.rlib', '.rmeta', '.pdb', '.exe', '.dll',
        '.exp', '.ilk', '.pch', '.res', '.tmp', '.rsp', '.spv', '.blbi', '.blri'
    ), [StringComparer]::OrdinalIgnoreCase)
    $pending = [Collections.Generic.Stack[IO.DirectoryInfo]]::new()
    $nativeSaved = $nativeBoundary + 'Saved'
    if ([IO.Directory]::Exists($nativeSaved)) { $pending.Push([IO.DirectoryInfo]::new($nativeSaved)) }
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        if (($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Reparse point redirects cleanup discovery: $($directory.FullName.Substring($nativeBoundary.Length))"
        }
        $prefix = $directory.FullName.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        if ([IO.File]::Exists($prefix + 'CACHEDIR.TAG') -or [IO.File]::Exists($prefix + '.rustc_info.json')) {
            $buildOutputs.Add($directory)
            continue
        }
        foreach ($entry in $directory.EnumerateFileSystemInfos()) {
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Reparse point redirects cleanup discovery: $($entry.FullName.Substring($nativeBoundary.Length))"
            }
            if ($entry -is [IO.DirectoryInfo]) {
                $pending.Push($entry)
            } elseif ($compiledExtensions.Contains($entry.Extension)) {
                $discovered.Add($entry.FullName.Substring($nativeBoundary.Length).Replace('\', '/'))
            }
        }
    }
    foreach ($directory in $buildOutputs) {
        $dist = $directory.FullName.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar + 'dist'
        if ([IO.Directory]::Exists($dist)) {
            foreach ($entry in $directory.EnumerateFileSystemInfos()) {
                if ($entry.Name -ne 'dist') { $discovered.Add($entry.FullName.Substring($nativeBoundary.Length).Replace('\', '/')) }
            }
        } else {
            $discovered.Add($directory.FullName.Substring($nativeBoundary.Length).Replace('\', '/'))
        }
    }
    $Path = $discovered.ToArray()
}

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
        if (-not $absolute.StartsWith($workspaceBoundary, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Path is outside this workspace: $relative"
        }
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
