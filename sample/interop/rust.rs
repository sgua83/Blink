#[allow(non_camel_case_types, dead_code)]
#[path = "../../target/interop/fractal.rs"]
mod fractal;
use std::time::Instant;

fn serial(pixels: &mut [u32], limit: u32) {
    for i in 0..pixels.len() as u32 {
        let cx = (i % 1000) as f32 / 1000.0 * 3.5 - 2.5;
        let cy = (i / 1000) as f32 / 1000.0 * 2.0 - 1.0;
        let mut x = 0.0;
        let mut y = 0.0;
        let mut iteration = 0;
        while x * x + y * y <= 4.0 && iteration < limit {
            let next_x = x * x - y * y + cx;
            y = 2.0 * x * y + cy;
            x = next_x;
            iteration += 1;
        }
        pixels[i as usize] = iteration;
    }
}
fn main() -> Result<(), String> {
    let backend = if std::env::args().nth(1).as_deref() == Some("gpu") {
        fractal::Backend::Gpu
    } else {
        fractal::Backend::Cpu
    };
    let mut expected = vec![0; fractal::PIXELS_COUNT];
    let mut pixels = vec![0; fractal::PIXELS_COUNT];
    let limit = 256;
    let begin = Instant::now();
    let mut program = fractal::Program::new(backend)?;
    let setup_ms = begin.elapsed().as_secs_f64() * 1000.0;
    let mut serial_samples = Vec::new();
    let mut blink_samples = Vec::new();
    for sample in -2..9 {
        let begin = Instant::now();
        serial(&mut expected, limit);
        let serial_ms = begin.elapsed().as_secs_f64() * 1000.0;
        let begin = Instant::now();
        program.run(&mut pixels, &limit)?;
        let blink_ms = begin.elapsed().as_secs_f64() * 1000.0;
        if sample >= 0 {
            serial_samples.push(serial_ms);
            blink_samples.push(blink_ms);
        }
    }
    serial_samples.sort_by(f64::total_cmp);
    blink_samples.sort_by(f64::total_cmp);
    let different = pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
    let max_difference = pixels
        .iter()
        .zip(&expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    let checksum: u64 = pixels.iter().map(|&x| u64::from(x)).sum();
    println!(
        "host=Rust backend={} pixels={} limit={limit}",
        if matches!(backend, fractal::Backend::Gpu) {
            "gpu"
        } else {
            "cpu"
        },
        pixels.len()
    );
    println!(
        "setup_ms={setup_ms:.3}\nserial_ms={:.3}\nblink_call_ms={:.3}\nspeedup={:.2}x",
        serial_samples[4],
        blink_samples[4],
        serial_samples[4] / blink_samples[4]
    );
    println!(
        "different_pixels={different} max_iteration_difference={max_difference} checksum={checksum}"
    );
    Ok(())
}
