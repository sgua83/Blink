//! 各示例复用的数据布局读写与 B 入口加载。在 Blink 仓库根目录运行。
pub mod particles;
pub mod scene;
pub mod sheet;

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

