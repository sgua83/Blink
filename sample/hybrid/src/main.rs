//! 《Blink 和 Rust 混合编程》配套宿主。在 Blink 仓库根目录运行。
mod particles;
mod scene;
mod sheet;

use blanguage::ExecutableProgram;
use blcompute::{Dataset, Result};
use std::{path::Path, time::Instant};

fn program(module: &str, entry: &str) -> Result<ExecutableProgram> {
    let root = Path::new("sample/hybrid/Programs");
    Ok(blanguage::compile_entry(&root.join(module), root, entry)?
        .programs
        .remove(0))
}

fn f32_at(data: &Dataset, row: usize, field: &str) -> Result<f32> {
    let i = row * data.stride() as usize / 4 + data.field_offset(field)? as usize / 4;
    Ok(f32::from_bits(data.words()[i]))
}

fn put(data: &mut Dataset, row: usize, field: &str, values: &[f32]) -> Result<()> {
    let i = row * data.stride() as usize / 4 + data.field_offset(field)? as usize / 4;
    for (word, value) in data.words_mut()[i..].iter_mut().zip(values) {
        *word = value.to_bits();
    }
    Ok(())
}

fn word(data: &mut Dataset, row: usize, field: &str, value: u32) -> Result<()> {
    let i = row * data.stride() as usize / 4 + data.field_offset(field)? as usize / 4;
    data.words_mut()[i] = value;
    Ok(())
}

fn elapsed(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("particles");
    let backend = args.get(1).map(String::as_str).unwrap_or("cpu");
    match command {
        "particles" => particles::run(
            backend,
            args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(3),
        ),
        "steps" => particles::steps(),
        "flow" => particles::flow(),
        "scene" => scene::run(backend),
        "sheet" => sheet::run(backend),
        "image" => sheet::image(),
        _ => Err(std::io::Error::other(
            "用法: particles cpu|gpu [数量] | steps | flow | scene cpu|gpu | sheet cpu|gpu | image",
        )
        .into()),
    }
}
