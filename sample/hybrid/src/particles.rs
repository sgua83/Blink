use crate::{elapsed, f32_at, program, put};
use blanguage::ExecutableProgram;
use blcompute::{Dataset, Result, Value, cpu::CpuProgram, vulkan::Device};
use std::time::Instant;

fn inputs(code: &ExecutableProgram, count: u32) -> Result<Vec<Dataset>> {
    let mut bodies = Dataset::new(code, 0, count)?;
    let stride = bodies.stride() as usize / 4;
    let position = bodies.field_offset("position")? as usize / 4;
    let velocity = bodies.field_offset("velocity")? as usize / 4;
    println!(
        "layout stride={} position={} velocity={} energy={}",
        bodies.stride(),
        position * 4,
        velocity * 4,
        bodies.field_offset("energy")?
    );
    for (i, row) in bodies.words_mut().chunks_exact_mut(stride).enumerate() {
        row[position] = (1.0 + (i % 1000) as f32 * 0.001).to_bits();
        row[velocity + 1] = 1.0_f32.to_bits();
    }
    Ok(vec![bodies, Dataset::new(code, 1, 1)?])
}

fn values() -> [Value; 2] {
    [Value::F32(0.01), Value::F32(0.2)]
}

pub fn run(backend: &str, count: u32) -> Result<()> {
    let setup = Instant::now();
    let code = program("simulation/statistics.b", "step_and_total")?;
    let data = inputs(&code, count)?;
    let result = match backend {
        "cpu" => {
            let code = CpuProgram::new(code)?;
            println!("setup_ms={:.3}", elapsed(setup));
            let mut data = data;
            let start = Instant::now();
            code.run(&mut data, &values())?;
            println!("run_wait_ms={:.3}", elapsed(start));
            data
        }
        "gpu" => {
            let device = Device::for_program(&code)?;
            println!("GPU {}", device.name());
            let code = device.compile(code)?;
            println!("setup_ms={:.3}", elapsed(setup));
            let transfer = Instant::now();
            let data = data
                .into_iter()
                .map(|d| device.upload(d))
                .collect::<Result<Vec<_>>>()?;
            println!("upload_enqueue_ms={:.3}", elapsed(transfer));
            let start = Instant::now();
            let done = code.submit(data, &values())?.wait()?;
            println!(
                "run_wait_ms={:.3} gpu_ms={:.3}",
                elapsed(start),
                done.timings.elapsed_ms
            );
            let start = Instant::now();
            let data = done
                .datasets()
                .iter()
                .map(|d| device.download(d))
                .collect::<Result<Vec<_>>>()?;
            println!("download_ms={:.3}", elapsed(start));
            data
        }
        _ => return Err(std::io::Error::other("后端须为 cpu 或 gpu").into()),
    };
    println!(
        "count={count} total_energy={:.6}",
        f32_at(&result[1], 0, "value")?
    );
    println!(
        "first position=({:.6},{:.6},{:.6}) energy={:.6}",
        f32_at(&result[0], 0, "position")?,
        f32::from_bits(result[0].words()[result[0].field_offset("position")? as usize / 4 + 1]),
        f32::from_bits(result[0].words()[result[0].field_offset("position")? as usize / 4 + 2]),
        f32_at(&result[0], 0, "energy")?
    );
    Ok(())
}

pub fn steps() -> Result<()> {
    let code = program("simulation/statistics.b", "step_and_total")?;
    let device = Device::for_program(&code)?;
    let data = inputs(&code, 3)?
        .into_iter()
        .map(|d| device.upload(d))
        .collect::<Result<Vec<_>>>()?;
    let code = device.compile(code)?;
    let first = code.submit(data, &values())?;
    let second = code.submit_after(first, &[0, 1], vec![], &values())?;
    let third = code.submit_after(second, &[0, 1], vec![], &values())?;
    let done = third.wait()?;
    let bodies = device.download(&done.datasets()[0])?;
    let total = device.download(&done.datasets()[1])?;
    println!("physical_steps=3 dt=0.01 simulated_seconds=0.03");
    println!(
        "first_x={:.6} first_y={:.6} total_energy={:.6}",
        f32_at(&bodies, 0, "position")?,
        f32::from_bits(bodies.words()[1]),
        f32_at(&total, 0, "value")?
    );
    Ok(())
}

pub fn flow() -> Result<()> {
    let code = program("simulation/statistics.b", "step_and_total")?;
    let finish = program("book/handoff.b", "finish")?;
    let device = Device::for_program(&code)?;
    let left = inputs(&code, 3)?
        .into_iter()
        .map(|d| device.upload(d))
        .collect::<Result<Vec<_>>>()?;
    let mut right_data = inputs(&code, 3)?;
    // 第二个群体的速度为前者两倍；能量因此约为四倍。
    for i in 0..3 {
        put(&mut right_data[0], i, "velocity", &[0.0, 2.0, 0.0])?;
    }
    let right = right_data
        .into_iter()
        .map(|d| device.upload(d))
        .collect::<Result<Vec<_>>>()?;
    let output = device.zeroed(&finish, 2, 1)?;
    let code = device.compile(code)?;
    let finish = device.compile(finish)?;
    let left = code.submit(left, &values())?;
    let right = code.submit(right, &values())?;
    let joined = finish
        .submit_join([left, right], &[1, 3, 4], vec![output], &[])?
        .wait()?;
    let data = joined
        .datasets()
        .iter()
        .map(|d| device.download(d))
        .collect::<Result<Vec<_>>>()?;
    println!(
        "left={:.6} right={:.6} combined={:.6}",
        f32_at(&data[0], 0, "value")?,
        f32_at(&data[1], 0, "value")?,
        f32_at(&data[2], 0, "value")?
    );
    Ok(())
}
