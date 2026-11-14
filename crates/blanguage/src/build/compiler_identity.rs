use std::{env, fs, path::Path, process::Command};

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

pub fn emit(bootstrap: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    sources(Path::new("src"), &mut files)?;
    files.extend([
        "build.rs".into(),
        "build-sdk.rs".into(),
        "Cargo.toml".into(),
        "../../Cargo.toml".into(),
    ]);
    files.sort();
    let mut identity = Digest::new();
    identity.field(bootstrap);
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        identity.field(path.to_str().ok_or("编译器源码路径不是UTF-8")?.as_bytes());
        identity.field(&fs::read(path)?);
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
        identity.field(name.as_bytes());
        match env::var_os(name) {
            Some(value) => {
                identity.field(b"present");
                identity.field(value.to_str().ok_or("构建配置不是UTF-8")?.as_bytes());
            }
            None => identity.field(b"absent"),
        }
    }
    let rustc = Command::new(env::var_os("RUSTC").ok_or("缺少RUSTC")?)
        .arg("-vV")
        .output()?;
    if !rustc.status.success() {
        return Err("无法取得编译器生产工具链身份".into());
    }
    identity.field(&rustc.stdout);
    println!(
        "cargo:rustc-env=BLINK_COMPILER_ID={}",
        hex(&identity.finish())
    );
    Ok(())
}

// The Cargo build script runs before the compiler can link its B archive.
// Consume fields in place; only the unfinished block crosses field boundaries.
struct Digest {
    state: [u32; 8],
    tail: [u8; 64],
    used: usize,
    length: u64,
}
impl Digest {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            tail: [0; 64],
            used: 0,
            length: 0,
        }
    }
    fn field(&mut self, bytes: &[u8]) {
        self.update(&(bytes.len() as u64).to_le_bytes());
        self.update(bytes);
    }
    fn update(&mut self, mut bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        if self.used != 0 {
            let count = (64 - self.used).min(bytes.len());
            self.tail[self.used..self.used + count].copy_from_slice(&bytes[..count]);
            self.used += count;
            bytes = &bytes[count..];
            if self.used != 64 {
                return;
            }
            compress(&mut self.state, &self.tail);
            self.used = 0;
        }
        let mut blocks = bytes.chunks_exact(64);
        for block in &mut blocks {
            compress(&mut self.state, block);
        }
        let remainder = blocks.remainder();
        self.tail[..remainder.len()].copy_from_slice(remainder);
        self.used = remainder.len();
    }
    fn finish(mut self) -> [u8; 32] {
        let mut tail = [0u8; 128];
        tail[..self.used].copy_from_slice(&self.tail[..self.used]);
        tail[self.used] = 0x80;
        let end = if self.used < 56 { 64 } else { 128 };
        tail[end - 8..end].copy_from_slice(&self.length.wrapping_mul(8).to_be_bytes());
        for block in tail[..end].chunks_exact(64) {
            compress(&mut self.state, block);
        }
        let mut result = [0; 32];
        for (bytes, word) in result.chunks_exact_mut(4).zip(self.state) {
            bytes.copy_from_slice(&word.to_be_bytes());
        }
        result
    }
}
fn compress(state: &mut [u32; 8], chunk: &[u8]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut w = [0u32; 64];
    for (word, bytes) in w.iter_mut().zip(chunk.chunks_exact(4)) {
        *word = u32::from_be_bytes(bytes.try_into().unwrap());
    }
    for i in 16..64 {
        let a = w[i - 15];
        let b = w[i - 2];
        w[i] = w[i - 16]
            .wrapping_add(a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3))
            .wrapping_add(w[i - 7])
            .wrapping_add(b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10));
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let t = h
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let u = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t);
        d = c;
        c = b;
        b = a;
        a = t.wrapping_add(u);
    }
    for (word, added) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *word = word.wrapping_add(added);
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
