//! Rust 原生 API；不经过 DLL 或 C ABI，保留程序检查和数据所有权。
use blcompute::{Dataset, cpu::CpuProgram, vulkan::Device};
use std::path::Path;
fn main() -> blcompute::Result<()> {
    let program = blanguage::compile_entry(
        Path::new("sample/lessons/map.b"),
        Path::new("sample"),
        "demo",
    )?
    .programs
    .remove(0);
    let data = Dataset::new(&program, 0, 4)?;
    let output = if std::env::args().nth(1).as_deref() == Some("gpu") {
        let device = Device::for_program(&program)?;
        let input = device.upload(data)?;
        let program = device.compile(program)?;
        let completed = program.submit(vec![input], &[])?.wait()?;
        device.download(&completed.datasets()[0])?
    } else {
        let program = CpuProgram::new(program)?;
        let mut data = vec![data];
        program.run(&mut data, &[])?;
        data.remove(0)
    };
    let values: Vec<_> = output.words().iter().map(|&x| f32::from_bits(x)).collect();
    println!("{values:?}");
    Ok(())
}
