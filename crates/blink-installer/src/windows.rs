use std::{ffi::{c_void, OsStr}, io, os::windows::ffi::OsStrExt, path::Path, ptr};
type Handle = *mut c_void;
#[link(name = "cabinet")]
unsafe extern "system" {
    fn CreateCompressor(algorithm: u32, allocation: *const c_void, handle: *mut Handle) -> i32;
    fn CloseCompressor(handle: Handle) -> i32;
    fn Compress(handle: Handle, input: *const u8, length: usize, output: *mut u8, capacity: usize, written: *mut usize) -> i32;
    fn CreateDecompressor(algorithm: u32, allocation: *const c_void, handle: *mut Handle) -> i32;
    fn CloseDecompressor(handle: Handle) -> i32;
    fn Decompress(handle: Handle, input: *const u8, length: usize, output: *mut u8, capacity: usize, written: *mut usize) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn ReplaceFileW(replaced: *const u16, replacement: *const u16, backup: *const u16, flags: u32, exclude: *const c_void, reserved: *const c_void) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(token: Handle, class: u32, data: *mut u32, size: u32, written: *mut u32) -> i32;
    fn RegOpenKeyExW(key: isize, name: *const u16, options: u32, access: u32, result: *mut isize) -> i32;
    fn RegQueryValueExW(key: isize, name: *const u16, reserved: *mut u32, kind: *mut u32, data: *mut u8, size: *mut u32) -> i32;
    fn RegSetValueExW(key: isize, name: *const u16, reserved: u32, kind: u32, data: *const u8, size: u32) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}
#[link(name = "shell32")]
unsafe extern "system" { fn ShellExecuteW(window: Handle, verb: *const u16, file: *const u16, parameters: *const u16, directory: *const u16, show: i32) -> isize; }
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: Handle, text: *const u16, caption: *const u16, flags: u32) -> i32;
    fn SendMessageTimeoutW(window: isize, message: u32, wparam: usize, lparam: isize, flags: u32, timeout: u32, result: *mut usize) -> isize;
}
fn wide(text: impl AsRef<OsStr>) -> Vec<u16> { text.as_ref().encode_wide().chain(Some(0)).collect() }
fn success(value: i32) -> io::Result<()> { if value == 0 { Err(io::Error::last_os_error()) } else { Ok(()) } }
fn registry(value: i32) -> io::Result<()> { if value == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(value)) } }
pub fn compress(input: &[u8]) -> io::Result<Vec<u8>> {
    unsafe {
        let mut handle = ptr::null_mut(); success(CreateCompressor(4, ptr::null(), &mut handle))?;
        let result = (|| {
            let mut length = 0;
            let status = Compress(handle, input.as_ptr(), input.len(), ptr::null_mut(), 0, &mut length);
            if status == 0 && io::Error::last_os_error().raw_os_error() != Some(122) { return Err(io::Error::last_os_error()); }
            let mut output = vec![0; length]; success(Compress(handle, input.as_ptr(), input.len(), output.as_mut_ptr(), output.len(), &mut length))?;
            output.truncate(length); Ok(output)
        })();
        let closed = success(CloseCompressor(handle)); result.and_then(|output| closed.map(|_| output))
    }
}
pub fn decompress(input: &[u8], length: usize) -> io::Result<Vec<u8>> {
    unsafe {
        let mut handle = ptr::null_mut(); success(CreateDecompressor(4, ptr::null(), &mut handle))?;
        let result = (|| {
            let mut output = vec![0; length]; let mut written = 0;
            success(Decompress(handle, input.as_ptr(), input.len(), output.as_mut_ptr(), output.len(), &mut written))?;
            if written != length { return Err(io::Error::other("解压长度与安装内容不符")); }
            Ok(output)
        })();
        let closed = success(CloseDecompressor(handle)); result.and_then(|output| closed.map(|_| output))
    }
}
pub fn elevated() -> io::Result<bool> {
    unsafe {
        let mut token = ptr::null_mut(); success(OpenProcessToken(GetCurrentProcess(), 8, &mut token))?;
        let mut elevation = 0; let mut size = 0;
        let result = success(GetTokenInformation(token, 20, &mut elevation, 4, &mut size));
        let closed = success(CloseHandle(token)); result?; closed?; Ok(elevation != 0)
    }
}
pub fn elevate() -> super::Result<()> {
    let file = wide(std::env::current_exe()?); let verb = wide("runas");
    let result = unsafe { ShellExecuteW(ptr::null_mut(), verb.as_ptr(), file.as_ptr(), ptr::null(), ptr::null(), 1) };
    if result <= 32 { return Err(io::Error::other(format!("Windows管理员授权未完成：{result}")).into()); }
    Ok(())
}
pub fn message(text: &str, failed: bool) {
    unsafe { MessageBoxW(ptr::null_mut(), wide(text).as_ptr(), wide("Blink 安装").as_ptr(), if failed { 16 } else { 64 }); }
}
pub fn replace(candidate: &Path, destination: &Path) -> io::Result<()> {
    unsafe { success(ReplaceFileW(wide(destination).as_ptr(), wide(candidate).as_ptr(), ptr::null(), 0, ptr::null(), ptr::null())) }
}
pub fn configure_path(binary: &Path) -> io::Result<()> {
    unsafe {
        let mut key = 0;
        registry(RegOpenKeyExW(0x80000002u32 as i32 as isize, wide("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment").as_ptr(), 0, 3, &mut key))?;
        let result = (|| {
            let name = wide("Path"); let mut kind = 0; let mut bytes = 0;
            registry(RegQueryValueExW(key, name.as_ptr(), ptr::null_mut(), &mut kind, ptr::null_mut(), &mut bytes))?;
            let mut value = vec![0u16; (bytes as usize).div_ceil(2)];
            registry(RegQueryValueExW(key, name.as_ptr(), ptr::null_mut(), &mut kind, value.as_mut_ptr().cast(), &mut bytes))?;
            if kind != 1 && kind != 2 { return Err(io::Error::other("机器PATH不是Windows字符串")); }
            value.truncate(bytes as usize / 2); if value.last() == Some(&0) { value.pop(); }
            let path = String::from_utf16(&value).map_err(io::Error::other)?;
            let binary = binary.to_str().ok_or_else(|| io::Error::other("安装路径不是Unicode"))?;
            if !path.split(';').next().is_some_and(|part| part.eq_ignore_ascii_case(binary)) {
                let remaining = path.split(';').filter(|part| !part.eq_ignore_ascii_case(binary)).collect::<Vec<_>>().join(";");
                let updated = wide(format!("{binary};{remaining}"));
                registry(RegSetValueExW(key, name.as_ptr(), 0, kind, updated.as_ptr().cast(), u32::try_from(updated.len() * 2).map_err(io::Error::other)?))?;
            }
            Ok(())
        })();
        let closed = registry(RegCloseKey(key)); result?; closed?;
        println!("PATH_CONFIGURED {}", binary.display()); Ok(())
    }
}
pub fn refresh_environment() -> io::Result<()> {
    let environment = wide("Environment"); let mut receipt = 0;
    // The ordinary desktop owner receives the notification after a LocalSystem install.
    let result = unsafe { SendMessageTimeoutW(0xffff, 0x1a, 0, environment.as_ptr() as isize, 2, 5000, &mut receipt) };
    if result == 0 { return Err(io::Error::other(format!("Windows未完成环境更新通知：{}", io::Error::last_os_error()))); }
    println!("ENVIRONMENT_REFRESHED"); Ok(())
}
