//! CLI 只导出编译器已经生成的完整程序映像，不另选集合／调度编译路径。
use blanguage::{
    ast::Target,
    ir::{Operation, Program},
    native::{self, ContinuationCode, NativeCode, StageCode as NativeStage},
    spirv,
};
use std::path::Path;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn native_file(path: &Path, stem: &str, code: &NativeCode, relocations: bool) -> Result<()> {
    std::fs::write(path.join(format!("{stem}.x64")), &code.bytes)?;
    if relocations {
        let text = code
            .relocations
            .iter()
            .map(|r| format!("{} {:?}\n", r.offset, r.target))
            .collect::<String>();
        std::fs::write(path.join(format!("{stem}.reloc")), text)?;
    }
    Ok(())
}
fn shader_file(path: &Path, stem: &str, words: &[u32]) -> Result<()> {
    let bytes = words
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect::<Vec<_>>();
    std::fs::write(path.join(format!("{stem}.spv")), bytes)?;
    Ok(())
}
pub fn program(program: &Program, out: &Path, stem: &str) -> Result<()> {
    if program.target() != Target::Cpu {
        let image = spirv::compile_program(program).map_err(std::io::Error::other)?;
        for (i, code) in image.stages().iter().enumerate() {
            let name = format!("{stem}.{i}");
            shader_file(out, &name, &code.main)?;
            for (suffix, words) in [
                ("tail", &code.tail),
                ("add", &code.scan_add),
                ("dispatch", &code.dispatch),
                ("selection", &code.selector),
            ] {
                if let Some(words) = words {
                    shader_file(out, &format!("{name}.{suffix}"), words)?;
                }
            }
            if let Some((suffix, words)) = &code.finish {
                shader_file(out, &format!("{name}.{suffix}"), words)?;
            }
            if let Some(sort) = &code.sort {
                for (suffix, shader) in ["scan", "tail", "add", "gather"]
                    .into_iter()
                    .zip(&sort[16..20])
                {
                    shader_file(out, &format!("{name}.{suffix}"), &shader.words)?;
                }
                for (key, shader) in sort[20..].iter().enumerate() {
                    shader_file(out, &format!("{name}.key-{key}"), &shader.words)?;
                }
                for shift in (0..32).step_by(4) {
                    for (suffix, index) in [("histogram", shift / 4), ("scatter", 8 + shift / 4)] {
                        shader_file(out, &format!("{name}.{shift}.{suffix}"), &sort[index].words)?;
                    }
                }
            }
        }
    }
    if program.target() != Target::Gpu {
        let image = native::compile_program(program).map_err(std::io::Error::other)?;
        for (i, code) in image.stages().iter().enumerate() {
            let name = format!("{stem}.{i}");
            match code {
                NativeStage::Element {
                    main, continuation, ..
                } => {
                    native_file(out, &name, main, true)?;
                    match continuation {
                        Some(ContinuationCode::ReductionTail(code)) => {
                            native_file(out, &format!("{name}.tail"), code, true)?
                        }
                        Some(ContinuationCode::ScanAdd(code)) => {
                            native_file(out, &format!("{name}.add"), code, false)?
                        }
                        Some(ContinuationCode::CompactScatter(code)) => {
                            native_file(out, &format!("{name}.scatter"), code, false)?
                        }
                        Some(ContinuationCode::SegmentedFinish(code)) => {
                            native_file(out, &format!("{name}.finish"), code, false)?
                        }
                        Some(ContinuationCode::Scan { prefix, apply, .. }) => {
                            native_file(out, &format!("{name}.prefix"), prefix, true)?;
                            let suffix = if matches!(
                                image.program().stages()[i].operation,
                                Operation::CustomScan(_)
                            ) {
                                "add"
                            } else {
                                "finish"
                            };
                            native_file(out, &format!("{name}.{suffix}"), apply, true)?;
                        }
                        None => {}
                    }
                }
                NativeStage::Sort(code) => {
                    native_file(out, &name, &code.initialize[0], true)?;
                    for (key, code) in code.initialize[1..].iter().rev().enumerate() {
                        native_file(out, &format!("{name}.key-{key}"), code, false)?;
                    }
                    for (suffix, code) in [
                        ("histogram", &code.histogram),
                        ("scatter", &code.scatter),
                        ("gather", &code.gather),
                    ] {
                        native_file(out, &format!("{name}.{suffix}"), code, false)?;
                    }
                }
            }
        }
    }
    std::fs::write(out.join(format!("{stem}.layout")), program.describe())?;
    Ok(())
}
