#[path = "src/build/compiler_identity.rs"]
mod compiler_identity;
use std::{env, fs, process::Command};

fn bootstrap_modules() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::PathBuf};
    println!("cargo:rerun-if-env-changed=BLINK_BOOTSTRAP_SDK");
    let seed = match env::var_os("BLINK_BOOTSTRAP_SDK") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env::var_os("ProgramFiles").ok_or("缺少ProgramFiles")?).join("Blink"),
    };
    // Keep the selected input until all bootstrap compilation and archiving ends.
    let _seed_use = OpenOptions::new().read(true).share_mode(1).open(seed.join("VERSION"))?;
    for name in ["VERSION", "bin/bc.exe", "bin/blink.exe"] {
        println!("cargo:rerun-if-changed={}", seed.join(name).display());
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("缺少OUT_DIR")?);
    let mut shared = Vec::new();
    for (name, source) in [("unicode_scalar", "../../library/blink/unicode_scalar"),
        ("path_core", "../../library/blink/windows/path_core")] {
        let object = out.join(format!("blink_compiler_{name}.obj"));
        for extension in ["b", "h"] {
            println!("cargo:rerun-if-changed={source}.{extension}");
        }
        let compiled = Command::new(seed.join("bin/bc.exe"))
            .arg("-c").arg(format!("{source}.b"))
            .args(["-I", "../../library", "--module", "-o"]).arg(&object).status()?;
        if !compiled.success() { return Err(format!("Blink种子{}编译{name}纯模块失败", seed.display()).into()); }
        shared.push(object);
    }
    let modules = ["lexer", "parser", "names", "signatures", "type_families", "type_layout", "scalar_fold", "archive", "coff", "metadata", "bindings", "coff_read", "metadata_read", "object_read", "ast_storage", "pe", "combine", "native_import", "native_resources", "instance_domain", "unwind", "frame_layout", "x64_instruction", "coloring", "call_layout", "spirv_envelope", "static_uses", "native_select", "host_sdk", "module_load", "stage_fusion", "source_bind", "type_reference", "c_function", "participation", "cli"];
    let objects: Vec<_> = modules.iter().map(|name| out.join(format!("blink_compiler_{name}.obj"))).collect();
    for (name, object) in modules.iter().zip(&objects) {
        let mut compiler = Command::new(seed.join("bin/bc.exe"));
        compiler.arg("-c").arg(format!("src/bootstrap/{name}.b"))
            .args(["-I", "src", "--module", "-o"]).arg(object);
        if *name == "host_sdk" {
            compiler.args(["-I", "../../library"]);
            for header in ["directory", "error", "time"] {
                println!("cargo:rerun-if-changed=../../library/blink/windows/{header}.h");
            }
        }
        if *name == "parser" { compiler.arg("--import").arg(&shared[0]); }
        if *name == "names" {
            compiler.args(["--instance", "sort_rows<u64>", "--instance", "sort_rows<u32>", "--instance", "sort_rows<array<u64,3>>"]);
        }
        let compiled = compiler.status()?;
        if !compiled.success() {
            return Err(format!("Blink种子{}编译{name}模块失败", seed.display()).into());
        }
    }
    let archive = out.join("blink_compiler_bootstrap.lib");
    let archived = Command::new(seed.join("bin/blink.exe"))
        .arg("--host-archive").args(&shared).args(&objects).arg("-o").arg(&archive).status()?;
    if !archived.success() {
        return Err(format!("Blink种子{}归档B引导模块失败", seed.display()).into());
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=blink_compiler_bootstrap");
    Ok(fs::read(archive)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("CARGO_FEATURE_COMPILER").is_none() {
        println!("cargo:rerun-if-changed=build.rs");
        return Ok(());
    }
    compiler_identity::emit(&bootstrap_modules()?)
}
