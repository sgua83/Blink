#![windows_subsystem = "windows"]
mod windows;
use std::{env, fs::{self, File, OpenOptions}, io::{self, Read, Write, Seek, SeekFrom, Cursor}, path::{Path, PathBuf, Component}, process::{Command, ExitCode}};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;

const VERSION: &str = include_str!("../../../VERSION");
const MAGIC: &[u8; 8] = b"BLNKEXE1";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn error(text: impl Into<String>) -> io::Error { io::Error::other(text.into()) }
fn number<const N: usize>(input: &mut impl Read) -> io::Result<[u8; N]> { let mut value = [0; N]; input.read_exact(&mut value)?; Ok(value) }
fn list(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() { list(root, &entry.path(), files)?; }
        else if entry.file_type()?.is_file() { files.push(entry.path().strip_prefix(root).unwrap().into()); }
        else { return Err(error("安装包只接受实际文件")); }
    }
    Ok(())
}
fn pack(root: &Path, output: &Path) -> Result<()> {
    let mut files = Vec::new(); list(root, root, &mut files)?; files.sort();
    let mut raw = Vec::new(); raw.write_all(&u32::try_from(files.len())?.to_le_bytes())?;
    for relative in &files {
        let name = relative.to_str().ok_or_else(|| error("安装文件名不是UTF-8"))?.replace('\\', "/");
        let data = fs::read(root.join(relative))?;
        raw.write_all(&u32::try_from(name.len())?.to_le_bytes())?; raw.write_all(name.as_bytes())?;
        raw.write_all(&(data.len() as u64).to_le_bytes())?; raw.write_all(&data)?;
    }
    let compressed = windows::compress(&raw)?;
    let mut executable = File::open(env::current_exe()?)?;
    let mut destination = File::create(output)?;
    let offset = io::copy(&mut executable, &mut destination)?;
    destination.write_all(&compressed)?; destination.write_all(MAGIC)?;
    destination.write_all(&offset.to_le_bytes())?;
    destination.write_all(&(raw.len() as u64).to_le_bytes())?;
    destination.write_all(&(compressed.len() as u64).to_le_bytes())?;
    destination.sync_all()?;
    println!("PACKED {} files={} bytes={}", output.display(), files.len(), destination.metadata()?.len());
    Ok(())
}
struct Entry { name: String, start: usize, length: usize }
struct Package { data: Vec<u8>, entries: Vec<Entry> }
impl Package {
    fn open() -> Result<Self> {
        let mut executable = File::open(env::current_exe()?)?;
        let end = executable.metadata()?.len(); executable.seek(SeekFrom::End(-32))?;
        if &number::<8>(&mut executable)? != MAGIC { return Err(error("安装程序没有Blink安装内容").into()); }
        let offset = u64::from_le_bytes(number(&mut executable)?);
        let raw = u64::from_le_bytes(number(&mut executable)?);
        let length = u64::from_le_bytes(number(&mut executable)?);
        if offset.checked_add(length).and_then(|n| n.checked_add(32)) != Some(end) { return Err(error("安装内容长度与程序不符").into()); }
        executable.seek(SeekFrom::Start(offset))?;
        let mut compressed = vec![0; usize::try_from(length)?]; executable.read_exact(&mut compressed)?;
        let data = windows::decompress(&compressed, usize::try_from(raw)?)?;
        let mut input = Cursor::new(data.as_slice());
        let count = u32::from_le_bytes(number(&mut input)?);
        let mut entries = Vec::new();
        for _ in 0..count {
            let length = u32::from_le_bytes(number(&mut input)?) as usize;
            let mut name = vec![0; length]; input.read_exact(&mut name)?;
            let name = String::from_utf8(name)?; relative_path(&name)?;
            let length = usize::try_from(u64::from_le_bytes(number(&mut input)?))?;
            let start = usize::try_from(input.position())?;
            let end = start.checked_add(length).ok_or_else(|| error("安装文件长度溢出"))?;
            if end > data.len() { return Err(error("安装文件超出包内容").into()); }
            input.set_position(end as u64); entries.push(Entry { name, start, length });
        }
        if input.position() != data.len() as u64 { return Err(error("安装内容存在未解释的数据").into()); }
        Ok(Self { data, entries })
    }
    fn file(&self, name: &str) -> Result<&[u8]> {
        let entry = self.entries.iter().find(|entry| entry.name == name).ok_or_else(|| error(format!("安装包缺少{name}")))?;
        Ok(&self.data[entry.start..entry.start + entry.length])
    }
}
fn relative_path(name: &str) -> io::Result<&Path> {
    let path = Path::new(name);
    if name.is_empty() || !path.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(error(format!("安装文件必须位于Blink目录内：{name}")));
    }
    Ok(path)
}
// Read the string array owned by the release record, without a second file inventory.
fn installed_files(record: &str) -> Result<Vec<String>> {
    let (_, value) = record.split_once("\"files\"").ok_or_else(|| error("安装记录缺少文件清单"))?;
    let (_, value) = value.split_once(':').ok_or_else(|| error("安装记录files无值"))?;
    let mut chars = value.trim_start().chars().peekable();
    if chars.next() != Some('[') { return Err(error("安装记录files不是数组").into()); }
    let mut result = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) { chars.next(); }
        if chars.peek() == Some(&']') { break; }
        if chars.next() != Some('"') { return Err(error("安装文件清单不是字符串").into()); }
        let mut text = String::new();
        loop {
            match chars.next().ok_or_else(|| error("安装文件名未结束"))? {
                '"' => break,
                '\\' => match chars.next().ok_or_else(|| error("安装文件名转义未结束"))? {
                    '"' => text.push('"'), '\\' => text.push('\\'), '/' => text.push('/'),
                    'b' => text.push('\u{8}'), 'f' => text.push('\u{c}'), 'n' => text.push('\n'), 'r' => text.push('\r'), 't' => text.push('\t'),
                    'u' => {
                        let digits = chars.by_ref().take(4).collect::<String>();
                        let first = u16::from_str_radix(&digits, 16)?;
                        if (0xd800..=0xdbff).contains(&first) {
                            if chars.next() != Some('\\') || chars.next() != Some('u') { return Err(error("安装文件名Unicode不完整").into()); }
                            let digits = chars.by_ref().take(4).collect::<String>();
                            let second = u16::from_str_radix(&digits, 16)?;
                            for value in char::decode_utf16([first, second]) { text.push(value?); }
                        } else { text.push(char::from_u32(first as u32).ok_or_else(|| error("安装文件名Unicode无效"))?); }
                    }
                    _ => return Err(error("安装文件名转义无效").into()),
                },
                c => text.push(c),
            }
        }
        relative_path(&text)?; result.push(text);
        while chars.peek().is_some_and(|c| c.is_whitespace()) { chars.next(); }
        match chars.peek() { Some(',') => { chars.next(); }, Some(']') => break, _ => return Err(error("安装文件数组未结束").into()) }
    }
    Ok(result)
}
fn install(package: &Package) -> Result<String> {
    let version = std::str::from_utf8(package.file("VERSION")?)?.trim();
    if version != VERSION.trim() { return Err(error("安装内容与安装程序版本不符").into()); }
    let root = PathBuf::from(env::var_os("ProgramFiles").ok_or_else(|| error("缺少ProgramFiles安装根"))?).join("Blink");
    let record = package.file("release.json")?;
    let record_text = std::str::from_utf8(record)?;
    let (_, commit) = record_text.split_once("\"git_commit\"").ok_or_else(|| error("安装记录缺少Git身份"))?;
    let (_, commit) = commit.split_once(':').ok_or_else(|| error("安装记录Git身份无值"))?;
    let record_path = root.join("release.json");
    let version_path = root.join("VERSION");
    if commit.trim_start().starts_with('"') && version_path.try_exists()? && record_path.try_exists()?
        && fs::read_to_string(&version_path)?.trim() == version && fs::read(&record_path)? == record {
        windows::configure_path(&root.join("bin"))?;
        println!("ALREADY_INSTALLED {version} {}", root.display());
        return Ok(format!("Blink {version} 已安装，系统PATH已配置。"));
    }
    fs::create_dir_all(&root)?;
    let lease = OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(4).open(&version_path)?;
    let empty = lease.metadata()?.len() == 0;
    let mut changed = false;
    let result = (|| -> Result<()> {
        let current = if record_path.try_exists()? { Some(fs::read(&record_path)?) } else { None };
        if let Some(current) = current {
            for name in installed_files(std::str::from_utf8(&current)?)? {
                if name == "VERSION" || package.entries.iter().any(|entry| entry.name == name) { continue; }
                let file = root.join(relative_path(&name)?);
                if file.is_file() { changed = true; fs::remove_file(file)?; }
            }
        }
        for entry in &package.entries {
            if entry.name == "VERSION" { continue; }
            let file = root.join(relative_path(&entry.name)?);
            fs::create_dir_all(file.parent().unwrap())?;
            changed = true; fs::write(file, &package.data[entry.start..entry.start + entry.length])?;
        }
        for name in ["bc", "blink", "brun"] {
            let output = Command::new(root.join(format!("bin/{name}.exe"))).arg("--version").creation_flags(0x08000000).output()?;
            let text = String::from_utf8(output.stdout)?;
            if !output.status.success() || !text.trim_end().ends_with(version) { return Err(error(format!("{name}未报告发行版本{version}")).into()); }
            print!("{text}");
        }
        windows::configure_path(&root.join("bin"))?;
        let candidate = root.join("VERSION.installing"); fs::write(&candidate, package.file("VERSION")?)?;
        windows::replace(&candidate, &version_path)?;
        println!("INSTALLED {version} {}", root.display());
        Ok(())
    })();
    if result.is_err() && (changed || empty) && version_path.exists() { fs::remove_file(&version_path)?; }
    drop(lease); result?;
    Ok(format!("Blink {version} 已安装到 {}。\n已配置系统 PATH，新终端可直接运行 bc、blink、brun。", root.display()))
}
fn run(args: &[String]) -> Result<()> {
    if args == ["--version"] { println!("Blink installer {}", VERSION.trim()); return Ok(()); }
    if args.first().is_some_and(|arg| arg == "--pack") && args.len() == 3 { return pack(Path::new(&args[1]), Path::new(&args[2])); }
    if args == ["--refresh-environment"] { return Ok(windows::refresh_environment()?); }
    let quiet = args == ["--quiet"];
    if !args.is_empty() && !quiet { return Err(error("使用：双击安装，或 --quiet 静默安装").into()); }
    if !windows::elevated()? {
        if quiet { return Err(error("静默安装需要由现有管理员入口启动").into()); }
        return windows::elevate();
    }
    let message = install(&Package::open()?)?;
    if !quiet { windows::refresh_environment()?; windows::message(&message, false); }
    Ok(())
}
fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(value) => {
            eprintln!("Blink installation failed: {value}");
            if args.is_empty() { windows::message(&format!("Blink 安装失败：{value}"), true); }
            ExitCode::FAILURE
        }
    }
}
