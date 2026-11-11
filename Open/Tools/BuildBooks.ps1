#requires -Version 7.2
param(
    [Parameter(Mandatory = $true)][string]$SourceRoot,
    [Parameter(Mandatory = $true)][string]$GitCommit,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$utf8 = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$root = [IO.Path]::GetFullPath($SourceRoot)
$output = [IO.Path]::GetFullPath($OutputDirectory)
$version = [IO.File]::ReadAllText((Join-Path $root 'VERSION')).Trim()
$edge = Join-Path ([Environment]::GetEnvironmentVariable('ProgramFiles(x86)')) 'Microsoft/Edge/Application/msedge.exe'
$sample = Join-Path $root 'sample'
$base = [Uri]::new($sample + [IO.Path]::DirectorySeparatorChar).AbsoluteUri
$css = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Books.css'))
$fonts = Join-Path $env:WINDIR 'Fonts'
foreach ($font in @(@('REGULAR', 'msyh.ttc'), @('BOLD', 'msyhbd.ttc'), @('CODE', 'consola.ttf'))) {
    $css = $css.Replace('{{FONT_' + $font[0] + '}}', [Uri]::new((Join-Path $fonts $font[1])).AbsoluteUri)
}
$books = @(
    @{ Source = '并行编程的艺术'; File = '并行编程的艺术-B语言' },
    @{ Source = 'Blink和Rust混合编程'; File = 'Blink和Rust混合编程' },
    @{ Source = 'B语言源码分析'; File = 'B语言源码分析' },
    @{ Source = 'BLEngine源码分析'; File = 'BLEngine源码分析' }
)
[IO.Directory]::CreateDirectory($output) | Out-Null
$layout = Join-Path $output 'layout'
[IO.Directory]::CreateDirectory($layout) | Out-Null
foreach ($book in $books) {
    $markdown = [IO.File]::ReadAllText((Join-Path $sample ($book.Source + '.md')))
    $heading = [regex]::Match($markdown, '\A# ([^\r\n]+)\r?\n')
    $title = [Net.WebUtility]::HtmlEncode($heading.Groups[1].Value)
    $body = (ConvertFrom-Markdown -InputObject $markdown.Substring($heading.Length)).Html
    $body = $body.Replace('<p>源码节选：', '<p class="source">源码节选：')
    $body = [regex]::Replace($body, '(?<=<a href=")[^"]+', {
        param($match)
        $href = [Net.WebUtility]::HtmlDecode($match.Value)
        if ($href.StartsWith('#') -or [Uri]::IsWellFormedUriString($href, [UriKind]::Absolute)) { return $match.Value }
        $target = [Uri]::new([Uri]$base, $href)
        $relative = [IO.Path]::GetRelativePath($root, $target.LocalPath).Replace('\', '/')
        $path = (($relative -split '/') | ForEach-Object { [Uri]::EscapeDataString($_) }) -join '/'
        [Net.WebUtility]::HtmlEncode("https://github.com/sgua83/Blink/blob/$GitCommit/$path" + $target.Query + $target.Fragment)
    })
    foreach ($mode in @('standard', 'mobile')) {
        $suffix = if ($mode -eq 'mobile') { '-手机阅读版' } else { '' }
        $edition = if ($mode -eq 'mobile') { '手机阅读版' } else { '标准版' }
        $pdf = Join-Path $output ($book.File + $suffix + '.pdf')
        $html = Join-Path $layout ($book.File + '-' + $mode + '.html')
        $profile = Join-Path $layout ($book.File + '-' + $mode + '-browser')
        $document = @"
<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><base href="$base"><title>$title</title><style>$css</style></head>
<body class="$mode"><section class="cover"><div class="eyebrow">BLINK · 技术书籍</div><h1>$title</h1><div class="subtitle">$edition · v$version</div><p class="identity">源码：$GitCommit</p></section><article>$body</article></body></html>
"@
        [IO.File]::WriteAllText($html, $document, $utf8)
        $start = [Diagnostics.ProcessStartInfo]::new($edge)
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        foreach ($argument in @('--headless', '--disable-gpu', '--no-pdf-header-footer',
            '--allow-file-access-from-files', "--user-data-dir=$profile", "--print-to-pdf=$pdf",
            [Uri]::new($html).AbsoluteUri)) { $start.ArgumentList.Add($argument) }
        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $start
        try {
            $process.Start() | Out-Null
            $stdout = $process.StandardOutput.ReadToEndAsync()
            $stderr = $process.StandardError.ReadToEndAsync()
            $process.WaitForExit()
            $stdout.GetAwaiter().GetResult() | Out-Null
            $errorText = $stderr.GetAwaiter().GetResult()
            if ($process.ExitCode -ne 0) { throw "Book renderer exited $($process.ExitCode): $errorText" }
        } finally { $process.Dispose() }
        Get-Item -LiteralPath $pdf
    }
}
