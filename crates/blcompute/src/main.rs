use blcompute::{Dataset, Result, Value, cpu::CpuProgram, vulkan::Device};
use std::{path::Path, time::Instant};

fn error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    std::io::Error::other(message.into()).into()
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        println!("Blink runner\nblrun <cpu|gpu> <module.b> <program> <module-root> <output-directory> <binding.bldata|:count>... [texture.bltex...] [-- value...]");
        return Ok(());
    }
    compute(&args)
}

fn compute(args: &[String]) -> Result<()> {
    use blanguage::{ast::Access, ir::Scalar};
    let backend = args.first().ok_or_else(|| error("缺少执行设备"))?;
    let module = Path::new(args.get(1).ok_or_else(|| error("缺少 .b 模块"))?);
    let name = args.get(2).ok_or_else(|| error("缺少 program 名称"))?;
    let root = Path::new(args.get(3).ok_or_else(|| error("缺少模块根目录"))?);
    let out = Path::new(args.get(4).ok_or_else(|| error("缺少输出目录"))?);
    let compilation = blanguage::compile_entry(module, root, name)?;
    let conditional = !compilation.templates.is_empty();
    let program = compilation
        .programs
        .into_iter()
        .next()
        .ok_or_else(|| {
            error(if conditional {
                format!("{name} 具有前缀域前置条件，需要具有域所有权的原生绑定，或通过满足条件的 program 组合成无前置条件入口")
            } else {
                format!("没有公开 program {name}")
            })
        })?;
    if !program.accelerations().is_empty()
        || program.buffers().iter().any(|b| b.ty.contains_views())
    {
        return Err(error(
            "blrun 文件入口接收普通值数据；加速结构和 GPU 引用通过原生 Bindings API 绑定",
        ));
    }
    let data_end = 5 + program.buffers().len();
    let end = data_end + program.texture_count();
    if args.len() < end {
        return Err(error(
            "每个 dataset 需要 .bldata／:元素数，每个 texture2d 需要 .bltex 文件",
        ));
    }
    let mut datasets = Vec::new();
    for (i, source) in args[5..data_end].iter().enumerate() {
        datasets.push(if let Some(count) = source.strip_prefix(':') {
            Dataset::new(&program, i, count.parse()?)?
        } else {
            Dataset::read_data(&program, i, Path::new(source))?
        });
    }
    let textures = args[data_end..end]
        .iter()
        .map(|p| blcompute::Texture2D::read(Path::new(p)))
        .collect::<Result<Vec<_>>>()?;
    let tail = &args[end..];
    let mut arguments = if let Some(first) = tail.first() {
        if first != "--" {
            return Err(error("值参数前需要 --"));
        }
        tail[1..].iter()
    } else {
        tail.iter()
    };
    let mut values = Vec::new();
    for uniform in program.uniforms() {
        for (scalar, _) in &uniform.ty.components {
            let value = arguments
                .next()
                .ok_or_else(|| error(format!("缺少参数 {}", uniform.name)))?;
            values.push(match scalar {
                Scalar::F32 => Value::F32(value.parse()?),
                Scalar::U32 => Value::U32(value.parse()?),
                Scalar::I32 => Value::I32(value.parse()?),
                Scalar::Bool => Value::Bool(value.parse()?),
                Scalar::U64 => Value::U64(value.parse()?),
                Scalar::I64 => Value::I64(value.parse()?),
            });
        }
    }
    if arguments.next().is_some() {
        return Err(error("多余的值参数"));
    }
    let outputs: Vec<_> = program
        .buffers()
        .iter()
        .enumerate()
        .filter(|(_, b)| b.access != Access::Read)
        .map(|(i, b)| (i, b.name.clone()))
        .collect();
    println!("{}", program.describe());
    let start = Instant::now();
    let result = match backend.as_str() {
        "cpu" => CpuProgram::new(program)?
            .run_owned(blcompute::cpu::Bindings { datasets, textures }, &values)?
            .into_datasets(),
        "gpu" => {
            let device = Device::for_program(&program)?;
            println!("GPU {}", device.name());
            let compiled = device.compile(program)?;
            let data = datasets
                .into_iter()
                .map(|d| device.upload(d))
                .collect::<Result<Vec<_>>>()?;
            let textures = textures
                .iter()
                .map(|t| device.upload_texture(t))
                .collect::<Result<Vec<_>>>()?;
            let done = compiled
                .submit(
                    blcompute::vulkan::Bindings {
                        datasets: data,
                        accelerations: Vec::new(),
                        textures,
                    },
                    &values,
                )?
                .wait()?;
            println!("GPU_program_ms={:.3}", done.timings.elapsed_ms);
            // 只读数据的所有权仍随完成对象释放，文件消费者只读回实际输出。
            std::fs::create_dir_all(out)?;
            for (i, name) in &outputs {
                device
                    .download(&done.datasets()[*i])?
                    .write_data(&out.join(format!("{name}.bldata")), 1)?;
            }
            println!(
                "compute_elapsed_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.0
            );
            return Ok(());
        }
        _ => return Err(error("执行设备只能为 cpu 或 gpu")),
    };
    std::fs::create_dir_all(out)?;
    for (i, name) in outputs {
        result[i].write_data(&out.join(format!("{name}.bldata")), 1)?;
    }
    println!(
        "compute_elapsed_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}
