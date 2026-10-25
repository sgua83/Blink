use std::{env, path::PathBuf, process::Command};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let status = Command::new("cl")
        .args([
            "/nologo",
            "/utf-8",
            "/O2",
            "/arch:AVX2",
            "/fp:precise",
            "/std:c++20",
            "/EHsc",
            "/MD",
            "/c",
            "serial.cpp",
        ])
        .arg(format!("/Fo{}", out.join("serial.obj").display()))
        .status()
        .expect("需要 MSVC x64 开发环境中的 cl");
    if !status.success() {
        panic!("C++ 编译失败");
    }
    let status = Command::new("lib")
        .arg("/nologo")
        .arg(format!("/OUT:{}", out.join("book_serial.lib").display()))
        .arg(out.join("serial.obj"))
        .status()
        .expect("需要 MSVC lib");
    if !status.success() {
        panic!("C++ 静态库生成失败");
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=book_serial");
    println!("cargo:rustc-link-lib=dylib=msvcprt");
    println!("cargo:rerun-if-changed=serial.cpp");
}
