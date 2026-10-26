use std::{path::PathBuf, process::ExitCode};
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!(
            "Blink compiler\nblc check <module.b> [module-root] [program<args>]\nblc build <module.b> <output-directory> [module-root] [program<args>]\nblc build-dll <module.b> <output.dll> [module-root] [program<args>]"
        );
        return Ok(());
    }
    let command = &args[0];
    let path = PathBuf::from(args.get(1).ok_or("缺少 .b 文件")?);
    let has_output = matches!(command.as_str(), "build" | "build-dll");
    let root = if has_output { args.get(3) } else { args.get(2) }
        .map(PathBuf::from)
        .unwrap_or_else(|| path.parent().unwrap().into());
    let selected = args.get(if has_output { 4 } else { 3 });
    let compilation = if let Some(entry) = selected {
        blanguage::compile_entry(&path, &root, entry)?
    } else if command == "check" {
        blanguage::check(&path, &root)?
    } else {
        blanguage::compile(&path, &root)?
    };
    if !compilation.open_generics.is_empty() {
        println!(
            "open const declarations (body checked on instantiation): {}",
            compilation.open_generics.join(", ")
        );
    }
    let programs: Vec<&blanguage::ir::Program> = compilation
        .programs
        .iter()
        .map(|p| &**p)
        .chain(compilation.templates.iter().map(|p| &**p))
        .collect();
    for p in &programs {
        print!("{}", p.describe());
    }
    match command.as_str() {
        "build-dll" => {
            if compilation.programs.len() != 1 || !compilation.templates.is_empty() {
                return Err(
                    "build-dll 需要一个无前置条件的具体 program；请在命令末尾选择入口".into(),
                );
            }
            blanguage::dll::build(
                &compilation.programs[0],
                &PathBuf::from(args.get(2).ok_or("缺少 DLL 输出路径")?),
            )?;
        }
        "check" => {
            if programs.is_empty() {
                println!("library {} checked", path.display());
            }
        }
        "build" => {
            let out = PathBuf::from(args.get(2).ok_or("缺少输出目录")?);
            std::fs::create_dir_all(&out)?;
            for (index, p) in programs.iter().enumerate() {
                // 类型的限定名保留在 layout；导出文件不使用 Windows 禁止的尖括号。
                let stem = if let Some((base, _)) = p.name().split_once('<') {
                    format!(
                        "{}.instance-{index}",
                        base.replace("::", "_").replace('.', "_")
                    )
                } else {
                    p.name().replace("::", "_").replace('.', "_")
                };
                for i in 0..p.stages().len() {
                    if p.target() != blanguage::ast::Target::Cpu {
                        let words = blanguage::spirv::emit(p, i).map_err(std::io::Error::other)?;
                        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                        std::fs::write(out.join(format!("{stem}.{i}.spv")), bytes)?;
                        use blanguage::{
                            ir::Operation,
                            spirv::{Pass, emit_pass},
                        };
                        let passes: &[(&str, Pass)] = match p.stages()[i].operation {
                            Operation::Reduce(_) | Operation::CustomReduce(_) => {
                                &[("tail", Pass::ReductionTail)]
                            }
                            Operation::SegmentedReduce(_) | Operation::CustomSegmentedReduce(_) => {
                                &[
                                    ("tail", Pass::ScanTail),
                                    ("add", Pass::ScanAdd),
                                    ("finish", Pass::ScanRootAdd),
                                ]
                            }
                            Operation::ScanExclusive(ref s) if s.needs_root_add(p) => &[
                                ("tail", Pass::ScanTail),
                                ("add", Pass::ScanAdd),
                                ("root-add", Pass::ScanRootAdd),
                            ],
                            Operation::CustomScan(ref s) if s.needs_root_add(p) => &[
                                ("tail", Pass::ScanTail),
                                ("add", Pass::ScanAdd),
                                ("root-add", Pass::ScanRootAdd),
                            ],
                            Operation::CustomScan(_) => {
                                &[("tail", Pass::ScanTail), ("add", Pass::ScanAdd)]
                            }
                            Operation::ScanExclusive(_) => {
                                &[("tail", Pass::ScanTail), ("add", Pass::ScanAdd)]
                            }
                            Operation::Compact(_) => &[
                                ("tail", Pass::ScanTail),
                                ("add", Pass::ScanAdd),
                                ("scatter", Pass::CompactScatter),
                            ],
                            Operation::StableSort(_) => &[
                                ("scan", Pass::SortScan),
                                ("tail", Pass::ScanTail),
                                ("add", Pass::ScanAdd),
                                ("gather", Pass::SortGather),
                            ],
                            Operation::Map if p.stages()[i].selection.is_some() => &[
                                ("selection", Pass::SelectionScan),
                                ("tail", Pass::ScanTail),
                                ("add", Pass::ScanAdd),
                                ("scatter", Pass::SelectionScatter),
                            ],
                            Operation::Map => &[],
                        };
                        let dispatch = if p.stages()[i].selection.is_some() {
                            Some(Pass::SelectionArguments)
                        } else {
                            p.stages()[i].prefix.map(Pass::DispatchArguments)
                        };
                        if let Some(pass) = dispatch {
                            let words = emit_pass(p, i, pass).map_err(std::io::Error::other)?;
                            let bytes: Vec<_> =
                                words.iter().flat_map(|w| w.to_le_bytes()).collect();
                            std::fs::write(out.join(format!("{stem}.{i}.dispatch.spv")), bytes)?;
                        }
                        for (name, pass) in passes {
                            let words = emit_pass(p, i, *pass).map_err(std::io::Error::other)?;
                            let bytes: Vec<_> =
                                words.iter().flat_map(|w| w.to_le_bytes()).collect();
                            std::fs::write(out.join(format!("{stem}.{i}.{name}.spv")), bytes)?;
                        }
                        if let Operation::StableSort(s) = &p.stages()[i].operation {
                            for key in 0..s.keys.len() - 1 {
                                let words = emit_pass(p, i, Pass::SortKey(key))
                                    .map_err(std::io::Error::other)?;
                                let bytes: Vec<_> =
                                    words.iter().flat_map(|w| w.to_le_bytes()).collect();
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.key-{key}.spv")),
                                    bytes,
                                )?;
                            }
                            for shift in (0..32).step_by(4) {
                                for (name, pass) in [
                                    ("histogram", Pass::SortHistogram(shift)),
                                    ("scatter", Pass::SortScatter(shift)),
                                ] {
                                    let words =
                                        emit_pass(p, i, pass).map_err(std::io::Error::other)?;
                                    let bytes: Vec<_> =
                                        words.iter().flat_map(|w| w.to_le_bytes()).collect();
                                    std::fs::write(
                                        out.join(format!("{stem}.{i}.{shift}.{name}.spv")),
                                        bytes,
                                    )?;
                                }
                            }
                        }
                    }
                    if p.target() != blanguage::ast::Target::Gpu {
                        let native =
                            blanguage::native::emit(p, i).map_err(std::io::Error::other)?;
                        std::fs::write(out.join(format!("{stem}.{i}.x64")), native.bytes)?;
                        if let blanguage::ir::Operation::ScanExclusive(r) = &p.stages()[i].operation
                        {
                            let add = blanguage::native::emit_scan_add(p, r);
                            std::fs::write(out.join(format!("{stem}.{i}.add.x64")), add.bytes)?;
                        }
                        if let blanguage::ir::Operation::Compact(c) = &p.stages()[i].operation {
                            let scatter = blanguage::native::emit_compact_scatter(p, c);
                            std::fs::write(
                                out.join(format!("{stem}.{i}.scatter.x64")),
                                scatter.bytes,
                            )?;
                        }
                        if let blanguage::ir::Operation::CustomReduce(_) = &p.stages()[i].operation
                        {
                            let tail = blanguage::native::emit_custom_reduce_tail(p, i)?;
                            std::fs::write(out.join(format!("{stem}.{i}.tail.x64")), tail.bytes)?;
                            let reloc = tail
                                .relocations
                                .iter()
                                .map(|r| format!("{} {:?}\n", r.offset, r.intrinsic))
                                .collect::<String>();
                            std::fs::write(out.join(format!("{stem}.{i}.tail.reloc")), reloc)?;
                        }
                        if let blanguage::ir::Operation::CustomScan(_) = &p.stages()[i].operation {
                            for (name, code) in [
                                ("prefix", blanguage::native::emit_custom_scan_prefix(p, i)?),
                                ("add", blanguage::native::emit_custom_scan_add(p, i)?),
                            ] {
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.{name}.x64")),
                                    code.bytes,
                                )?;
                                let reloc = code
                                    .relocations
                                    .iter()
                                    .map(|r| format!("{} {:?}\n", r.offset, r.intrinsic))
                                    .collect::<String>();
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.{name}.reloc")),
                                    reloc,
                                )?;
                            }
                        }
                        if let blanguage::ir::Operation::CustomSegmentedReduce(_) =
                            &p.stages()[i].operation
                        {
                            for (name, code) in [
                                (
                                    "prefix",
                                    blanguage::native::emit_custom_segmented_prefix(p, i)?,
                                ),
                                (
                                    "finish",
                                    blanguage::native::emit_custom_segmented_finish(p, i)?,
                                ),
                            ] {
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.{name}.x64")),
                                    code.bytes,
                                )?;
                                let reloc = code
                                    .relocations
                                    .iter()
                                    .map(|r| format!("{} {:?}\n", r.offset, r.intrinsic))
                                    .collect::<String>();
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.{name}.reloc")),
                                    reloc,
                                )?;
                            }
                        }
                        if let blanguage::ir::Operation::SegmentedReduce(r) =
                            &p.stages()[i].operation
                        {
                            let finish = blanguage::native::emit_segmented_reduce_finish(p, r);
                            std::fs::write(
                                out.join(format!("{stem}.{i}.finish.x64")),
                                finish.bytes,
                            )?;
                        }
                        if let blanguage::ir::Operation::StableSort(s) = &p.stages()[i].operation {
                            for key in 0..s.keys.len() - 1 {
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.key-{key}.x64")),
                                    blanguage::native::emit_sort_key(p, s, key).bytes,
                                )?;
                            }
                            for (name, code) in [
                                ("histogram", blanguage::native::emit_sort_histogram(p)),
                                ("scatter", blanguage::native::emit_sort_scatter(p)),
                                ("gather", blanguage::native::emit_sort_gather(p, s)),
                            ] {
                                std::fs::write(
                                    out.join(format!("{stem}.{i}.{name}.x64")),
                                    code.bytes,
                                )?;
                            }
                        }
                        let reloc = native
                            .relocations
                            .iter()
                            .map(|r| format!("{} {:?}\n", r.offset, r.intrinsic))
                            .collect::<String>();
                        std::fs::write(out.join(format!("{stem}.{i}.reloc")), reloc)?;
                    }
                }
                std::fs::write(out.join(format!("{stem}.layout")), p.describe())?;
            }
        }
        _ => return Err(format!("未知命令 {command}").into()),
    }
    Ok(())
}
