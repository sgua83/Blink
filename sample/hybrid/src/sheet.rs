use crate::{elapsed, f32_at, program};
use blcompute::{
    Dataset, Result, Texture2D, TextureMip,
    cpu::{self, CpuProgram},
    vulkan::{self, Device},
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Instant,
};

fn texture() -> Result<Texture2D> {
    Texture2D::new(vec![TextureMip {
        width: 2,
        height: 2,
        pixels: vec![
            [0.9, 0.3, 0.1, 1.0],
            [0.1, 0.6, 0.9, 1.0],
            [0.2, 0.7, 0.3, 1.0],
            [0.8, 0.7, 0.2, 1.0],
        ],
    }])
}

pub fn run(backend: &str) -> Result<()> {
    let setup = Instant::now();
    let code = program("book/sheet.b", "materials<10000,1000>")?;
    let points = Dataset::new(&code, 0, 10_000_000)?;
    let albedo = texture()?;
    let output = match backend {
        "cpu" => {
            let code = CpuProgram::new(code)?;
            println!("setup_ms={:.3}", elapsed(setup));
            let start = Instant::now();
            let output = code.run_owned(
                cpu::Bindings {
                    datasets: vec![points],
                    textures: vec![albedo.clone()],
                },
                &[],
            )?;
            println!("first_run_wait_ms={:.3}", elapsed(start));
            let start = Instant::now();
            let output = code.run_owned(
                cpu::Bindings {
                    datasets: output,
                    textures: vec![albedo],
                },
                &[],
            )?;
            println!("resident_run_wait_ms={:.3}", elapsed(start));
            output.into_datasets().remove(0)
        }
        "gpu" => {
            let device = Device::for_program(&code)?;
            println!("GPU {}", device.name());
            let code = device.compile(code)?;
            let points = device.upload(points)?;
            let albedo = device.upload_texture(&albedo)?;
            println!("setup_upload_enqueue_ms={:.3}", elapsed(setup));
            let start = Instant::now();
            let done = code
                .submit(
                    vulkan::Bindings {
                        datasets: vec![points],
                        accelerations: vec![],
                        textures: vec![albedo.clone()],
                    },
                    &[],
                )?
                .wait()?;
            println!(
                "first_run_wait_ms={:.3} gpu_ms={:.3}",
                elapsed(start),
                done.timings.elapsed_ms
            );
            let start = Instant::now();
            let done = code
                .submit(
                    vulkan::Bindings {
                        datasets: done.into_bindings(),
                        accelerations: vec![],
                        textures: vec![albedo],
                    },
                    &[],
                )?
                .wait()?;
            println!(
                "resident_run_wait_ms={:.3} gpu_ms={:.3}",
                elapsed(start),
                done.timings.elapsed_ms
            );
            let start = Instant::now();
            let output = device.download(&done.datasets()[0])?;
            println!("points_download_ms={:.3}", elapsed(start));
            output
        }
        _ => return Err(std::io::Error::other("后端须为 cpu 或 gpu").into()),
    };
    let i = 500 * 10000 + 5000;
    let offset = i * output.stride() as usize / 4 + output.field_offset("radiance")? as usize / 4;
    println!(
        "count=10000000 center_radiance=({:.6},{:.6},{:.6})",
        f32_at(&output, i, "radiance")?,
        f32::from_bits(output.words()[offset + 1]),
        f32::from_bits(output.words()[offset + 2])
    );
    Ok(())
}

pub fn image() -> Result<()> {
    let setup = Instant::now();
    let code = program("book/sheet.b", "image")?;
    let device = Device::for_program(&code)?;
    println!("GPU {}", device.name());
    let data = (0..3)
        .map(|i| device.zeroed(&code, i, 1_000_000))
        .collect::<Result<Vec<_>>>()?;
    let albedo = device.upload_texture(&texture()?)?;
    let code = device.compile(code)?;
    println!("setup_enqueue_ms={:.3}", elapsed(setup));
    let start = Instant::now();
    let done = code
        .submit(
            vulkan::Bindings {
                datasets: data,
                accelerations: vec![],
                textures: vec![albedo],
            },
            &[],
        )?
        .wait()?;
    println!(
        "image_run_wait_ms={:.3} gpu_ms={:.3}",
        elapsed(start),
        done.timings.elapsed_ms
    );
    let start = Instant::now();
    let display = device.download(&done.datasets()[2])?;
    println!(
        "display_download_ms={:.3} display_bytes={}",
        elapsed(start),
        display.words().len() * 4
    );
    std::fs::create_dir_all("Saved/HybridBook")?;
    let mut file = BufWriter::new(File::create("Saved/HybridBook/materials.ppm")?);
    file.write_all(b"P6\n1000 1000\n255\n")?;
    for &bgra in display.words() {
        file.write_all(&[(bgra >> 16) as u8, (bgra >> 8) as u8, bgra as u8])?;
    }
    file.flush()?;
    println!("image=Saved/HybridBook/materials.ppm size=1000x1000");
    Ok(())
}
