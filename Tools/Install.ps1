#requires -Version 5.1
param(
    [string]$Version = 'latest',
    [string]$DownloadDirectory = (Join-Path (Get-Location) 'target/dist'),
    [string]$ApplyPackage
)
$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$repository = 'sgua83/Blink'
$assetName = 'Blink-windows-x64.zip'

if (-not $ApplyPackage) {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $endpoint = if ($Version -eq 'latest') { 'latest' } else {
        'tags/' + [Uri]::EscapeDataString("v$Version")
    }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/$endpoint"
    $currentRecord = Join-Path $env:ProgramFiles 'Blink/release.json'
    $currentVersion = Join-Path $env:ProgramFiles 'Blink/VERSION'
    if ((Test-Path -LiteralPath $currentRecord) -and (Test-Path -LiteralPath $currentVersion)) {
        $current = Get-Content -LiteralPath $currentRecord -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($current.git_commit -eq $release.target_commitish -and
            ('v' + [IO.File]::ReadAllText($currentVersion).Trim()) -eq $release.tag_name) {
            Write-Output "ALREADY_INSTALLED $($current.version) $currentRecord"
            return
        }
    }
    $asset = @($release.assets | Where-Object { $_.name -eq $assetName })
    if ($asset.Count -ne 1) { throw "Release $($release.tag_name) does not contain $assetName." }
    [IO.Directory]::CreateDirectory($DownloadDirectory) | Out-Null
    $ApplyPackage = Join-Path ([IO.Path]::GetFullPath($DownloadDirectory)) $assetName
    Invoke-WebRequest -UseBasicParsing -Uri $asset[0].browser_download_url -OutFile $ApplyPackage
    Write-Output "DOWNLOADED $($release.tag_name) $ApplyPackage"
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Download complete. Apply this package through the existing administrator maintenance entry: -ApplyPackage `"$ApplyPackage`"."
}
$ApplyPackage = [IO.Path]::GetFullPath($ApplyPackage)
$install = Join-Path $env:ProgramFiles 'Blink'
$versionFile = Join-Path $install 'VERSION'
[IO.Directory]::CreateDirectory($install) | Out-Null
$inputLease = $null
$packageArchive = $null
$emptyInput = $false
$changed = $false
$installed = $false
try {
    # VERSION is the actual SDK input held by compiler sessions and the installation owner.
    $inputLease = [IO.FileStream]::new($versionFile, [IO.FileMode]::OpenOrCreate,
        [IO.FileAccess]::ReadWrite, [IO.FileShare]::Delete)
    $emptyInput = $inputLease.Length -eq 0
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $packageArchive = [IO.Compression.ZipFile]::OpenRead($ApplyPackage)
    $reader = [IO.StreamReader]::new($packageArchive.GetEntry('release.json').Open(), $utf8)
    try { $record = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    $reader = [IO.StreamReader]::new($packageArchive.GetEntry('VERSION').Open(), $utf8)
    try { $versionText = $reader.ReadToEnd() } finally { $reader.Dispose() }
    $packageVersion = $versionText.Trim()
    if ($packageVersion -ne $record.version) { throw 'Package VERSION and release identity differ.' }
    $previous = @()
    $oldRecord = Join-Path $install 'release.json'
    if (Test-Path -LiteralPath $oldRecord) {
        $previous = @((Get-Content -LiteralPath $oldRecord -Raw -Encoding UTF8 | ConvertFrom-Json).files)
    }
    foreach ($relative in @($record.files) + $previous) {
        $destination = [IO.Path]::GetFullPath((Join-Path $install $relative))
        if (-not $destination.StartsWith($install + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Package file escapes the Blink installation: $relative"
        }
    }
    foreach ($relative in $previous) {
        if ($relative -eq 'VERSION' -or $relative -in $record.files) { continue }
        $destination = Join-Path $install $relative
        if (Test-Path -LiteralPath $destination -PathType Leaf) {
            $changed = $true
            Remove-Item -LiteralPath $destination
        }
    }
    foreach ($relative in $record.files) {
        if ($relative -eq 'VERSION') { continue }
        $source = $packageArchive.GetEntry($relative.Replace('\', '/')).Open()
        $destination = Join-Path $install $relative
        [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
        $changed = $true
        try {
            $output = [IO.File]::Create($destination)
            try { $source.CopyTo($output) } finally { $output.Dispose() }
        } finally { $source.Dispose() }
    }
    $changed = $true
    foreach ($tool in @('bc', 'blink', 'brun')) {
        $output = & (Join-Path $install "bin/$tool.exe") --version
        if ($LASTEXITCODE -ne 0 -or "$output" -notmatch ([regex]::Escape($packageVersion) + '$')) {
            throw "Installed $tool does not report package version $packageVersion."
        }
        Write-Output $output
    }
    $binaryPath = Join-Path $install 'bin'
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    if ($binaryPath -notin ($machinePath -split ';')) {
        [Environment]::SetEnvironmentVariable('Path', ($machinePath.TrimEnd(';') + ';' + $binaryPath), 'Machine')
    }
    # Expose the new SDK only after every installed component has been handed over.
    $newVersion = Join-Path $install 'VERSION.installing'
    [IO.File]::WriteAllText($newVersion, $versionText, $utf8)
    [IO.File]::Replace($newVersion, $versionFile, [NullString]::Value)
    if ($inputLease) { $inputLease.Dispose(); $inputLease = $null }
    $installed = $true
    Write-Output "INSTALLED $packageVersion P4 $($record.source_change) $install"
} catch {
    # A partial installation must not expose a usable SDK identity.
    if (($changed -or $emptyInput) -and (Test-Path -LiteralPath $versionFile)) { [IO.File]::Delete($versionFile) }
    throw
} finally {
    if ($inputLease) { $inputLease.Dispose() }
    if ($packageArchive) { $packageArchive.Dispose() }
    if ($installed) { Remove-Item -LiteralPath $ApplyPackage }
}
