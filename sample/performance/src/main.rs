use blanguage::{ExecutableProgram, ast::Access, ir::Scalar};
use blcompute::{Dataset, Result, Texture2D, TextureMip, Value, cpu::CpuProgram, vulkan::Device};
use std::{
    hint::black_box,
    path::{Path, PathBuf},
    time::Instant,
};

unsafe extern "C" {
    fn book_serial(
        kind: u32,
        n: u32,
        data: *const *mut u32,
        stride: *const u32,
        field: *const u32,
        texture: *const f32,
    );
}

fn copies(program: &ExecutableProgram, data: &[Dataset]) -> Result<Vec<Dataset>> {
    data.iter()
        .enumerate()
        .map(|(i, d)| Dataset::from_words(program, i, d.words().to_vec()))
        .collect()
}
fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
fn offset(data: &Dataset, name: &str) -> usize {
    data.field_offset(name).unwrap() as usize / 4
}
fn put(data: &mut Dataset, i: usize, at: usize, values: &[f32]) {
    let begin = i * data.stride() as usize / 4 + at;
    for (k, v) in values.iter().enumerate() {
        data.words_mut()[begin + k] = v.to_bits();
    }
}
fn difference<T: std::borrow::Borrow<Dataset>>(
    program: &ExecutableProgram,
    a: &[Dataset],
    b: &[T],
) -> (f64, u64) {
    let (mut absolute, mut integers) = (0.0f64, 0u64);
    for ((binding, a), b) in program.buffers().iter().zip(a).zip(b) {
        let b = b.borrow();
        if binding.access == Access::Read {
            continue;
        }
        let stride = a.stride() as usize / 4;
        for i in 0..a.count() as usize {
            for (scalar, at) in &binding.ty.components {
                let j = i * stride + *at as usize / 4;
                if *scalar == Scalar::F32 {
                    let x = f32::from_bits(a.words()[j]) as f64;
                    let y = f32::from_bits(b.words()[j]) as f64;
                    absolute = absolute.max((x - y).abs());
                } else {
                    integers += u64::from(a.words()[j] != b.words()[j]);
                }
            }
        }
    }
    (absolute, integers)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let name = args.get(1).map(String::as_str).unwrap_or("map");
    let default_count = match name {
        "map" | "scan" | "compact" | "algebra" => 100_000_000,
        "resolve" => 1_000_000,
        _ => 10_000_000,
    };
    let n: u32 = args
        .get(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(default_count);
    let samples = 9;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let lessons = root.join("sample");
    let engine = root.parent().unwrap().join("BLEngine/Programs");
    let (kind, module, entry, counts, module_root): (u32, &str, &str, Vec<u32>, &Path) = match name
    {
        "map" => (0, "lessons/map.b", "scale", vec![n], &lessons),
        "reduce" => (1, "lessons/scan.b", "sum", vec![n, 1], &lessons),
        "scan" => (2, "lessons/scan.b", "offsets", vec![n, n], &lessons),
        "compact" => (3, "lessons/pipeline.b", "select", vec![n, n, 1], &lessons),
        "pipeline" => (
            4,
            "lessons/pipeline.b",
            "run",
            vec![n, n, n, 1, n, 1],
            &lessons,
        ),
        "tiles" => (5, "lessons/tiles.b", "analyze<32>", vec![n], &lessons),
        "publish" => (6, "lessons/tiles.b", "publish<32>", vec![n], &lessons),
        "algebra" => (7, "lessons/algebra.b", "chain", vec![n, n, 1], &lessons),
        "bodies" => (
            8,
            "simulation/bodies.b",
            "step<BLBody<float3,3>>",
            vec![n],
            &engine,
        ),
        "body_total" => (
            12,
            "simulation/statistics.b",
            "step_and_total",
            vec![n, 1],
            &engine,
        ),
        "transforms" => (9, "scene/transforms.b", "prepare", vec![n, n], &engine),
        "shade" => (10, "material/textured.b", "shade", vec![n], &engine),
        "resolve" => (
            11,
            "render/resolve.b",
            "resolve<1000,1000,2,1000000,4000000>",
            vec![4_000_000, 1_000_000],
            &engine,
        ),
        _ => return Err(std::io::Error::other("未知案例").into()),
    };
    let n = if kind == 11 { 1_000_000 } else { n };
    let program = blanguage::compile_entry(&module_root.join(module), module_root, entry)?
        .programs
        .into_iter()
        .next()
        .unwrap();
    eprintln!("{}", program.describe());
    let mut initial: Vec<_> = counts
        .iter()
        .enumerate()
        .map(|(i, &n)| Dataset::new(&program, i, n))
        .collect::<Result<_>>()?;
    let mut field = Vec::<u32>::new();
    let texture_pixels: Vec<[f32; 4]> = (0..65536)
        .map(|i| {
            [
                (i % 256) as f32 / 512.0 + 0.2,
                (i / 256) as f32 / 512.0 + 0.2,
                0.35,
                1.0,
            ]
        })
        .collect();
    let texture = Texture2D::new(vec![TextureMip {
        width: 256,
        height: 256,
        pixels: texture_pixels.clone(),
    }])?;
    if kind == 8 || kind == 12 {
        field = ["position", "velocity", "energy"]
            .iter()
            .map(|f| offset(&initial[0], f) as u32)
            .collect();
    } else if kind == 10 {
        field = [
            "normal",
            "view",
            "light",
            "uv",
            "tint",
            "roughness",
            "ior",
            "radiance",
        ]
        .iter()
        .map(|f| offset(&initial[0], f) as u32)
        .collect();
    }
    for i in 0..initial[0].count() as usize {
        match kind {
            0 => put(&mut initial[0], i, 0, &[(i % 17) as f32 * 0.125]),
            1 | 2 => initial[0].words_mut()[i] = [3, 1, 4, 2][i % 4],
            3 | 4 => initial[0].words_mut()[i * 4..i * 4 + 4].copy_from_slice(&[
                i as u32,
                (i as u32 * 7 + 2) % 3,
                (i % 7 + 1) as u32,
                u32::from(i % 2 == 0),
            ]),
            5 | 6 => put(&mut initial[0], i, 0, &[(i % 17 + 1) as f32]),
            7 => initial[0].words_mut()[i * 2..i * 2 + 2].copy_from_slice(&[1, (i % 7 + 1) as u32]),
            8 | 12 => {
                put(
                    &mut initial[0],
                    i,
                    field[0] as usize,
                    &[
                        0.5 + (i % 97) as f32 / 97.0,
                        0.25 + (i % 31) as f32 / 31.0,
                        0.75,
                    ],
                );
                put(&mut initial[0], i, field[1] as usize, &[0.01, 0.02, 0.03]);
            }
            9 => {
                let x = (i % 17) as f32 * 0.01;
                put(
                    &mut initial[0],
                    i,
                    0,
                    &[
                        1.0 + x,
                        0.1,
                        0.0,
                        0.5,
                        0.0,
                        1.5 + x,
                        0.1,
                        0.25,
                        0.1,
                        0.0,
                        2.0 + x,
                        0.0,
                    ],
                );
            }
            10 => {
                put(&mut initial[0], i, field[0] as usize, &[0.0, 0.0, 1.0]);
                put(&mut initial[0], i, field[1] as usize, &[0.0, 0.0, 1.0]);
                put(&mut initial[0], i, field[2] as usize, &[0.6, 0.0, 0.8]);
                put(
                    &mut initial[0],
                    i,
                    field[3] as usize,
                    &[
                        ((i % 256) as f32 + 0.7) / 256.0,
                        ((i / 256 % 256) as f32 + 0.3) / 256.0,
                    ],
                );
                put(&mut initial[0], i, field[4] as usize, &[0.8, 0.6, 0.4]);
                put(
                    &mut initial[0],
                    i,
                    field[5] as usize,
                    &[0.2 + (i % 16) as f32 * 0.03],
                );
                put(&mut initial[0], i, field[6] as usize, &[1.5]);
            }
            11 => put(
                &mut initial[0],
                i,
                0,
                &[
                    0.1 + (i % 97) as f32 * 0.01,
                    0.2 + (i % 53) as f32 * 0.02,
                    0.5,
                ],
            ),
            _ => {}
        }
    }
    let values = match kind {
        6 => vec![Value::F32(2.0)],
        8 | 12 => vec![Value::F32(0.001), Value::F32(0.5)],
        _ => vec![],
    };
    let mut serial = copies(&program, &initial)?;
    let stride: Vec<_> = serial.iter().map(|d| d.stride() / 4).collect();
    let pointers: Vec<_> = serial
        .iter_mut()
        .map(|d| d.words_mut().as_mut_ptr())
        .collect();
    let mut cxx_times = Vec::new();
    for repeat in 0..samples + 2 {
        let start = Instant::now();
        unsafe {
            book_serial(
                kind,
                n,
                pointers.as_ptr(),
                stride.as_ptr(),
                field.as_ptr(),
                texture_pixels.as_ptr().cast(),
            );
        }
        let elapsed = ms(start);
        black_box(&serial);
        if repeat >= 2 {
            cxx_times.push(elapsed);
        }
    }
    let mut cpu: Vec<blcompute::cpu::CpuDataset> = copies(&program, &initial)?
        .into_iter()
        .map(Into::into)
        .collect();
    let mut cpu_times = Vec::new();
    let mut cpu_error = (0.0, 0);
    if kind != 11 {
        let compiled = CpuProgram::new(program.clone())?;
        for repeat in 0..samples + 2 {
            let start = Instant::now();
            cpu = compiled
                .run_owned(
                    blcompute::cpu::Bindings {
                        datasets: cpu,
                        textures: if kind == 10 {
                            vec![texture.clone()]
                        } else {
                            vec![]
                        },
                    },
                    &values,
                )?
                .into_datasets();
            let elapsed = ms(start);
            black_box(&cpu);
            if repeat >= 2 {
                cpu_times.push(elapsed);
            }
        }
        cpu_error = difference(&program, &serial, &cpu);
    }
    let prepare = Instant::now();
    let device = Device::for_program(&program)?;
    eprintln!("GPU {}", device.name());
    let compiled = device.compile(program.clone())?;
    let textures = if kind == 10 {
        vec![device.upload_texture(&texture)?]
    } else {
        vec![]
    };
    let mut gpu = initial
        .into_iter()
        .map(|d| device.upload(d))
        .collect::<Result<Vec<_>>>()?;
    let prepare_ms = ms(prepare);
    let (mut gpu_times, mut device_times) = (Vec::new(), Vec::new());
    for repeat in 0..samples + 2 {
        let start = Instant::now();
        let done = compiled
            .submit(
                blcompute::vulkan::Bindings {
                    datasets: gpu,
                    accelerations: vec![],
                    textures: textures.clone(),
                },
                &values,
            )?
            .wait()?;
        let elapsed = ms(start);
        if repeat >= 2 {
            gpu_times.push(elapsed);
            device_times.push(done.timings.elapsed_ms);
        }
        gpu = done.into_datasets();
    }
    let downloaded = gpu
        .iter()
        .map(|d| device.download(d))
        .collect::<Result<Vec<_>>>()?;
    let gpu_error = difference(&program, &serial, &downloaded);
    if kind == 12 {
        let at = field[2] as usize;
        let stride = serial[0].stride() as usize / 4;
        let reference: f64 = (0..n as usize)
            .map(|i| f32::from_bits(serial[0].words()[i * stride + at]) as f64)
            .sum();
        eprintln!(
            "total_energy: C++={} B_CPU={} B_GPU={} f64_reference={}",
            f32::from_bits(serial[1].words()[0]),
            f32::from_bits(cpu[1].words()[0]),
            f32::from_bits(downloaded[1].words()[0]),
            reference
        );
    }
    // 数据往返另行测量，不能与驻留计算的时长混为一谈。
    let mut roundtrip_times = Vec::new();
    for _ in 0..samples {
        let host = copies(&program, &serial)?;
        let start = Instant::now();
        let uploaded = host
            .into_iter()
            .map(|d| device.upload(d))
            .collect::<Result<Vec<_>>>()?;
        let done = compiled
            .submit(
                blcompute::vulkan::Bindings {
                    datasets: uploaded,
                    accelerations: vec![],
                    textures: textures.clone(),
                },
                &values,
            )?
            .wait()?;
        for (i, binding) in program.buffers().iter().enumerate() {
            if binding.access != Access::Read {
                black_box(device.download(&done.datasets()[i])?);
            }
        }
        roundtrip_times.push(ms(start));
    }
    let cxx = median(&mut cxx_times);
    let cpu = if cpu_times.is_empty() {
        f64::NAN
    } else {
        median(&mut cpu_times)
    };
    let wall = median(&mut gpu_times);
    println!(
        "case,n,samples,cxx_ms,b_cpu_ms,b_gpu_wall_ms,gpu_device_ms,gpu_roundtrip_ms,gpu_prepare_ms,cpu_speedup,gpu_speedup,cpu_max_abs,gpu_max_abs,cpu_integer_mismatch,gpu_integer_mismatch"
    );
    println!(
        "{name},{n},{samples},{cxx:.6},{cpu:.6},{wall:.6},{:.6},{:.6},{prepare_ms:.6},{:.3},{:.3},{:.9},{:.9},{},{}",
        median(&mut device_times),
        median(&mut roundtrip_times),
        cxx / cpu,
        cxx / wall,
        cpu_error.0,
        gpu_error.0,
        cpu_error.1,
        gpu_error.1
    );
    Ok(())
}
