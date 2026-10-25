use blcompute::{
    Result, Texture2D, TextureMip,
    vulkan::{Bindings, Device},
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

fn main() -> Result<()> {
    let setup = Instant::now();
    let root = Path::new("sample/hybrid/Programs");
    let mut compiled = blanguage::compile_entry(&root.join("book/sheet.b"), root, "image")?;
    let code = compiled.programs.remove(0);
    let device = Device::for_program(&code)?;
    println!("GPU {}", device.name());
    let points = device.zeroed(&code, 0, 1_000_000)?;
    let pixels = device.zeroed(&code, 1, 1_000_000)?;
    let display = device.zeroed(&code, 2, 1_000_000)?;
    let albedo = Texture2D::new(vec![TextureMip {
        width: 2,
        height: 2,
        pixels: vec![
            [0.9, 0.3, 0.1, 1.0],
            [0.1, 0.6, 0.9, 1.0],
            [0.2, 0.7, 0.3, 1.0],
            [0.8, 0.7, 0.2, 1.0],
        ],
    }])?;
    let albedo = device.upload_texture(&albedo)?;
    let program = device.compile(code)?;
    println!(
        "setup_enqueue_ms={:.3}",
        setup.elapsed().as_secs_f64() * 1000.0
    );

    let start = Instant::now();
    let done = program
        .submit(
            Bindings {
                datasets: vec![points, pixels, display],
                accelerations: vec![],
                textures: vec![albedo],
            },
            &[],
        )?
        .wait()?;
    println!(
        "image_run_wait_ms={:.3} gpu_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0,
        done.timings.elapsed_ms
    );

    let start = Instant::now();
    let display = device.download(&done.datasets()[2])?;
    println!(
        "display_download_ms={:.3} display_bytes={}",
        start.elapsed().as_secs_f64() * 1000.0,
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
