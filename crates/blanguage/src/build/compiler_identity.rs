use std::{env, fs::{self, File}, io::{self, Read, Write}, path::Path, process::Command};

fn sources(path: &Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

pub fn emit(mut identity: Identity) -> Result<(), Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    sources(Path::new("src"), &mut files)?;
    files.extend([
        "build.rs".into(),
        "build-sdk.rs".into(),
        "Cargo.toml".into(),
        "../../Cargo.toml".into(),
    ]);
    files.sort();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        identity.field(path.to_str().ok_or("编译器源码路径不是UTF-8")?.as_bytes())?;
        identity.field(&fs::read(path)?)?;
    }
    println!("cargo:rerun-if-changed=src");
    // Cargo owns these inputs; the content identity uses their effective values.
    for name in [
        "TARGET",
        "HOST",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_FEATURE_INSTALLED",
        "CARGO_FEATURE_COMPILER",
    ] {
        identity.field(name.as_bytes())?;
        match env::var_os(name) {
            Some(value) => {
                identity.field(b"present")?;
                identity.field(value.to_str().ok_or("构建配置不是UTF-8")?.as_bytes())?;
            }
            None => identity.field(b"absent")?,
        }
    }
    let rustc = Command::new(env::var_os("RUSTC").ok_or("缺少RUSTC")?)
        .arg("-vV")
        .output()?;
    if !rustc.status.success() {
        return Err("无法取得编译器生产工具链身份".into());
    }
    identity.field(&rustc.stdout)?;
    println!(
        "cargo:rustc-env=BLINK_COMPILER_ID={}",
        hex(&identity.finish()?)
    );
    Ok(())
}


use std::{ffi::c_void, ptr};
type HashHandle = *mut c_void;

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptCreateHash(algorithm: HashHandle, hash: *mut HashHandle,
        object: *mut u8, object_bytes: u32, secret: *mut u8, secret_bytes: u32, flags: u32) -> i32;
    fn BCryptHashData(hash: HashHandle, bytes: *const u8, length: u32, flags: u32) -> i32;
    fn BCryptFinishHash(hash: HashHandle, result: *mut u8, length: u32, flags: u32) -> i32;
    fn BCryptDestroyHash(hash: HashHandle) -> i32;
}

fn status(operation: &str, status: i32) -> io::Result<()> {
    if status >= 0 { Ok(()) }
    else { Err(io::Error::other(format!("{operation} failed with NTSTATUS {status:#010x}"))) }
}

pub struct Identity(HashHandle);
impl Identity {
    pub fn new() -> io::Result<Self> {
        const SHA256: HashHandle = 0x41usize as HashHandle;
        let mut hash = ptr::null_mut();
        status("BCryptCreateHash", unsafe {
            BCryptCreateHash(SHA256, &mut hash, ptr::null_mut(), 0, ptr::null_mut(), 0, 0)
        })?;
        Ok(Self(hash))
    }

    pub fn library(&mut self, input: &mut File, mut output: impl Write) -> io::Result<()> {
        let mut remaining = input.metadata()?.len();
        self.update(&remaining.to_le_bytes())?;
        let mut buffer = [0; 64 * 1024];
        while remaining != 0 {
            let count = remaining.min(buffer.len() as u64) as usize;
            let bytes = &mut buffer[..count];
            input.read_exact(bytes)?;
            output.write_all(bytes)?;
            self.update(bytes)?;
            remaining -= count as u64;
        }
        Ok(())
    }

    fn field(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.update(&(bytes.len() as u64).to_le_bytes())?;
        self.update(bytes)
    }

    fn update(&mut self, bytes: &[u8]) -> io::Result<()> {
        for part in bytes.chunks(u32::MAX as usize) {
            status("BCryptHashData", unsafe { BCryptHashData(self.0, part.as_ptr(), part.len() as u32, 0) })?;
        }
        Ok(())
    }

    fn finish(self) -> io::Result<[u8; 32]> {
        let mut result = [0; 32];
        status("BCryptFinishHash", unsafe {
            BCryptFinishHash(self.0, result.as_mut_ptr(), result.len() as u32, 0)
        })?;
        Ok(result)
    }
}
impl Drop for Identity {
    fn drop(&mut self) { unsafe { BCryptDestroyHash(self.0); } }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
