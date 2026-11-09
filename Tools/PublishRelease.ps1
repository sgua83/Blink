#requires -Version 7.2
param(
    [string]$P4Client,
    [switch]$BuildOnly,
    [string]$Supervisor,
    [string]$MaintenanceConfig,
    [string]$NodeRegistry,
    [string[]]$MaintenanceTargets = @()
)
$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$env:GIT_TERMINAL_PROMPT = '0'
$env:GCM_INTERACTIVE = 'Never'
$env:P4CHARSET = 'utf8'
$repository = 'sgua83/Blink'
$scriptRoot = Split-Path $PSScriptRoot -Parent

function Native {
    param([string]$File, [string[]]$Arguments, [Text.Encoding]$StandardOutputEncoding = $utf8)
    $start = [Diagnostics.ProcessStartInfo]::new($File)
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WorkingDirectory = (Get-Location).ProviderPath
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = $StandardOutputEncoding
    $start.StandardErrorEncoding = $utf8
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        $process.Start() | Out-Null
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $output = ($stdout.GetAwaiter().GetResult() + "`n" + $stderr.GetAwaiter().GetResult()).TrimEnd()
        if ($process.ExitCode -ne 0) { throw "$File exited $($process.ExitCode)`: $output" }
        $output
    } finally { $process.Dispose() }
}
function ReadP4 {
    param([string[]]$Arguments)
    $arguments = @('-ztag', '-Mj') + $Arguments
    if ($P4Client) { $arguments = @('-c', $P4Client) + $arguments }
    foreach ($line in (Native p4 $arguments) -split "`n") {
        if ($line) {
            $item = $line | ConvertFrom-Json
            if ($item.severity -ge 3) { throw $item.data }
            $item
        }
    }
}
function InstallNodes {
    param([string]$Package)
    $targets = @($MaintenanceTargets)
    if ($NodeRegistry) {
        $clients = @(ReadP4 @('clients', '-e', 'BLanguage_*'))
        $page = $NodeRegistry
        while ($page) {
            $nodes = Invoke-RestMethod -Uri $page
            foreach ($node in $nodes.nodes) {
                $owned = @($clients | Where-Object { $_.Owner -eq $node.node_id -or $_.Host -eq $node.node_id })
                $workspaces = @($node.workspace_proofs | Where-Object { $_.project_id -eq 'BLanguage' })
                if ($owned.Count -gt 0 -or $workspaces.Count -gt 0) {
                    $targets += "$($node.node_id)@$($node.unit_control_host)"
                }
            }
            $page = $null
            if ($null -ne $nodes.next_shard -and $nodes.next_node_id) {
                $separator = if ($NodeRegistry.Contains('?')) { '&' } else { '?' }
                $page = $NodeRegistry + $separator + 'shard=' + $nodes.next_shard + '&after=' + [Uri]::EscapeDataString($nodes.next_node_id)
            }
        }
    }
    $targets = @($targets | Select-Object -Unique)
    if ($targets.Count -eq 0) { return }
    if (-not $Supervisor) {
        $owner = Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort 29083 -State Listen
        $Supervisor = (Get-CimInstance Win32_Process -Filter "ProcessId=$($owner.OwningProcess)").ExecutablePath
    }
    $name = 'Blink-Setup-windows-x64.exe'
    $failures = @()
    foreach ($node in $targets) {
        try {
            $configuration = @()
            if ($MaintenanceConfig) { $configuration = @('--remote-config', $MaintenanceConfig) }
            # The existing maintenance transport owns distribution; nodes need no Internet proxy.
            $temporary = Native $Supervisor (@('exec', $node, 'cmd.exe /d /u /c echo %TEMP%') + $configuration) ([Text.Encoding]::Unicode)
            if ($temporary -notmatch '(?m)^([A-Za-z]:\\[^\r\n]+)\r?$') { throw 'The node did not return its temporary directory.' }
            $packagePath = Join-Path $Matches[1] $name
            Native $Supervisor (@('put', $node, $package, $packagePath) + $configuration) | Write-Output
            Native $Supervisor (@('system-exec', $node, ('"' + $packagePath + '" --quiet')) + $configuration) | Write-Output
            $complete = '"' + $packagePath + '" --refresh-environment && del "' + $packagePath + '"'
            Native $Supervisor (@('exec', $node, $complete) + $configuration) | Write-Output
        } catch {
            $failures += "$node`: $($_.Exception.Message)"
            Write-Output "INSTALL_FAILED $node`: $($_.Exception.Message)"
        }
    }
    if ($failures.Count -gt 0) { throw ($failures -join "`n") }
}
Push-Location $scriptRoot
$lock = $null
$transcript = $false
try {
    $info = @(ReadP4 @('info'))[0]
    $P4Client = $info.clientName
    $root = $info.clientRoot
    $metadata = Native cargo @('metadata', '--no-deps', '--format-version', '1', '--manifest-path', (Join-Path $root 'Cargo.toml')) | ConvertFrom-Json
    $target = $metadata.target_directory
    $dist = Join-Path $target 'dist'
    $archive = Join-Path $dist 'Blink-Setup-windows-x64.exe'
    [IO.Directory]::CreateDirectory($dist) | Out-Null
    $lock = [IO.FileStream]::new((Join-Path $dist 'release.lock'), [IO.FileMode]::OpenOrCreate,
        [IO.FileAccess]::ReadWrite, [IO.FileShare]::None, 4096, [IO.FileOptions]::DeleteOnClose)
    Start-Transcript -Path (Join-Path $dist 'release.log') -Append | Out-Null
    $transcript = $true
    $mirror = Join-Path $root 'Saved/GitPublish/Mirror.git'
    $source = Join-Path $dist 'source'
    $sourceZip = Join-Path $dist 'source.zip'
    if (-not $BuildOnly) {
        Native (Get-Process -Id $PID).Path @('-NoProfile', '-NonInteractive', '-File',
            (Join-Path $PSScriptRoot 'PublishGit.ps1'), '-P4Client', $P4Client, '-MirrorDirectory', $mirror) | Write-Output
        $remote = Native git @('--git-dir', $mirror, 'ls-remote', 'origin', 'refs/heads/main')
        $commit = ($remote -split '\s+')[0]
        $message = Native git @('--git-dir', $mirror, 'log', '-1', '--format=%B', $commit)
        if ($message -notmatch '(?m)^P4-Change: (\d+)\s*$') { throw 'Published source has no P4 identity.' }
        $change = [int]$Matches[1]
        $version = (Native git @('--git-dir', $mirror, 'show', "${commit}:VERSION")).Trim()
        $tag = "v$version"
        $api = "https://api.github.com/repos/$repository"
        $headers = @{ Accept = 'application/vnd.github+json' }
        # The release owner must also see unpublished drafts from earlier invocations.
        $credential = "protocol=https`nhost=github.com`n`n" | & git credential fill
        if ($LASTEXITCODE -ne 0) { throw 'GitHub credential lookup failed.' }
        $secret = ($credential | Where-Object { $_.StartsWith('password=') }).Substring(9)
        $headers.Authorization = "Bearer $secret"
        $credential = $null; $secret = $null
        $releases = Invoke-RestMethod -Uri "$api/releases?per_page=100" -Headers $headers
        $published = @($releases | Where-Object { -not $_.draft -and -not $_.prerelease })
        if ($published.Count -gt 0 -and $published[0].tag_name -eq $tag) {
            Write-Output "UNCHANGED RELEASE $tag"
            if ($NodeRegistry -or $MaintenanceTargets.Count -gt 0) {
                Invoke-WebRequest -Uri "https://github.com/$repository/releases/download/$tag/Blink-Setup-windows-x64.exe" -OutFile $archive
                Write-Output "DOWNLOADED $tag $archive"
                InstallNodes $archive
                Remove-Item -LiteralPath $archive
            }
            return
        }
        Write-Output "SOURCE P4 $change $commit VERSION $version"
    } else {
        # A development package makes no submitted-source or publication claim.
        $source = $root
        $sourceZip = $null
        $change = $null
        $commit = $null
        $version = [IO.File]::ReadAllText((Join-Path $source 'VERSION')).Trim()
        Write-Output "WORKING TREE $source VERSION $version (not a formal release)"
    }
    $package = Join-Path $dist 'package'
    $books = Join-Path $dist 'books'
    $staging = @($package)
    if (-not $BuildOnly) { $staging += @($source, $books) }
    foreach ($directory in $staging) {
        if (Test-Path -LiteralPath $directory) {
            $resolved = [IO.Path]::GetFullPath($directory)
            if (-not $resolved.StartsWith([IO.Path]::GetFullPath($dist) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Release staging is outside this target/dist.'
            }
            Remove-Item -LiteralPath $resolved -Recurse
        }
        [IO.Directory]::CreateDirectory($directory) | Out-Null
    }
    if (-not $BuildOnly) {
    $inputs = @('Cargo.toml', 'Cargo.lock', 'VERSION', 'LICENSE', 'crates', 'library',
        'Tools/BuildBooks.ps1', 'Tools/Books.css', 'sample/*.md', 'sample/figures')
    $configuration = Native git @('--git-dir', $mirror, 'ls-tree', '--name-only', $commit, '.cargo', 'rust-toolchain', 'rust-toolchain.toml')
    $inputs += @($configuration -split "`n" | Where-Object { $_ })
    Native git (@('--git-dir', $mirror, 'archive', '--format=zip', '-o', $sourceZip, $commit) + $inputs) | Out-Null
    [IO.Compression.ZipFile]::ExtractToDirectory($sourceZip, $source)
    }
    Native cargo @('build', '--workspace', '--release', '--locked', '--features', 'blanguage/installed',
        '--manifest-path', (Join-Path $source 'Cargo.toml'), '--target-dir', $target) | Write-Output
    Native cargo @('rustc', '--release', '--locked', '--package', 'blink-installer', '--features', 'windows-installer',
        '--manifest-path', (Join-Path $source 'Cargo.toml'), '--target-dir', $target,
        '--', '-C', 'target-feature=+crt-static') | Write-Output
    [IO.Directory]::CreateDirectory((Join-Path $package 'bin')) | Out-Null
    foreach ($tool in @('bc', 'blink', 'brun')) {
        $executable = Join-Path $target "release/$tool.exe"
        $display = Native $executable @('--version')
        if ($display -notmatch ([regex]::Escape($version) + '$')) { throw "$tool is not the snapshot version $version." }
        [IO.File]::Copy($executable, (Join-Path $package "bin/$tool.exe"))
    }
    foreach ($directory in @('crates', 'library')) {
        Copy-Item -LiteralPath (Join-Path $source $directory) -Destination $package -Recurse
    }
    foreach ($file in @('Cargo.toml', 'Cargo.lock', 'VERSION', 'LICENSE')) {
        [IO.File]::Copy((Join-Path $source $file), (Join-Path $package $file))
    }
    foreach ($configuration in @('.cargo', 'rust-toolchain', 'rust-toolchain.toml')) {
        $path = Join-Path $source $configuration
        if (Test-Path -LiteralPath $path) { Copy-Item -LiteralPath $path -Destination $package -Recurse }
    }
    # P4's source-file attributes are workspace metadata, not package ownership.
    Get-ChildItem -LiteralPath $package -File -Recurse | ForEach-Object { $_.IsReadOnly = $false }
    $files = @(Get-ChildItem -LiteralPath $package -File -Recurse | ForEach-Object {
        [IO.Path]::GetRelativePath($package, $_.FullName).Replace('\', '/')
    }) + @('release.json')
    $record = [ordered]@{ version = $version; source_change = $change; git_commit = $commit;
        target = 'x86_64-pc-windows-msvc'; profile = 'release'; files = $files }
    [IO.File]::WriteAllText((Join-Path $package 'release.json'), ($record | ConvertTo-Json -Depth 4), $utf8)
    if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive }
    Native (Join-Path $target 'release/blink-setup.exe') @('--pack', $package, $archive) | Write-Output
    Write-Output "BUILT $archive P4 $change VERSION $version"
    if ($BuildOnly) { return }

    $bookPdfs = @(& (Join-Path $source 'Tools/BuildBooks.ps1') -SourceRoot $source -GitCommit $commit -OutputDirectory $books)
    $bookArchive = Join-Path $dist 'Blink-Books.zip'
    if (Test-Path -LiteralPath $bookArchive) { Remove-Item -LiteralPath $bookArchive }
    Compress-Archive -LiteralPath @($bookPdfs.FullName) -DestinationPath $bookArchive -CompressionLevel Optimal
    Write-Output "BOOKS $bookArchive P4 $change VERSION $version"

    $existing = @($releases | Where-Object { $_.tag_name -eq $tag })
    if ($existing.Count -gt 0) {
        if (-not $existing[0].draft) { throw "Release $tag already exists." }
        Invoke-RestMethod -Method Delete -Uri "$api/releases/$($existing[0].id)" -Headers $headers | Out-Null
    }
    $body = "P4-Change: $change`nGit-Commit: $commit`n`nWindows x64原生自解压安装EXE，双击安装或由管理员入口执行--quiet。自动配置系统PATH，安装到%ProgramFiles%/Blink，只保留当前版。包内为Release工具与源码SDK；运行框架编译仍需要Rust MSVC及Windows构建工具，GPU使用系统Vulkan驱动。当前能力与限制见本提交的Docs/BLanguage.md。另附Blink-Books.zip，包含同次已提交书稿生成的四本中文书籍标准版与手机版PDF。"
    $draft = Invoke-RestMethod -Method Post -Uri "$api/releases" -Headers $headers -ContentType 'application/json; charset=utf-8' -Body (
        @{tag_name=$tag;target_commitish=$commit;name="Blink $version";body=$body;draft=$true;prerelease=$false} | ConvertTo-Json)
    $upload = ($draft.upload_url -split '\{')[0]
    foreach ($asset in @($archive, $bookArchive)) {
        $name = [Uri]::EscapeDataString([IO.Path]::GetFileName($asset))
        Invoke-RestMethod -Method Post -Uri "${upload}?name=$name" -Headers $headers -ContentType 'application/octet-stream' -InFile $asset | Out-Null
    }
    $release = Invoke-RestMethod -Method Patch -Uri "$api/releases/$($draft.id)" -Headers $headers -ContentType 'application/json' -Body '{"draft":false,"make_latest":"true"}'
    Write-Output "PUBLISHED $($release.html_url) P4 $change VERSION $version"
    $headers.Clear()
    InstallNodes $archive
    foreach ($directory in @($source, $package, $books)) { Remove-Item -LiteralPath $directory -Recurse }
    Remove-Item -LiteralPath $sourceZip
    Remove-Item -LiteralPath $archive
    Remove-Item -LiteralPath $bookArchive
} finally {
    if ($transcript) { Stop-Transcript | Out-Null }
    if ($lock) { $lock.Dispose() }
    Pop-Location
}
