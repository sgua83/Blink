use crate::{elapsed, f32_at, program, put, word};
use blcompute::{Dataset, Result, cpu::CpuProgram, vulkan::Device};
use std::time::Instant;

pub fn run(backend: &str) -> Result<()> {
    let code = program("scene/components.b", "evaluate")?;
    let mut nodes = Dataset::new(&code, 0, 3)?;
    // 一个无环场景：根 -> 子节点 -> 孙节点。
    for i in 0..3 {
        put(
            &mut nodes,
            i,
            "local.r0",
            &[1.0, 0.0, 0.0, if i == 0 { 10.0 } else { 0.0 }],
        )?;
        put(
            &mut nodes,
            i,
            "local.r1",
            &[0.0, 1.0, 0.0, if i == 1 { 2.0 } else { 0.0 }],
        )?;
        put(
            &mut nodes,
            i,
            "local.r2",
            &[0.0, 0.0, 1.0, if i == 2 { 3.0 } else { 0.0 }],
        )?;
        word(&mut nodes, i, "active", 1)?;
        word(&mut nodes, i, "has_parent", (i != 0) as u32)?;
        word(&mut nodes, i, "parent", [0, 0, 1][i])?;
    }
    let worlds = Dataset::new(&code, 1, 3)?;
    let mut cameras = Dataset::new(&code, 2, 1)?;
    word(&mut cameras, 0, "node", 2)?;
    put(&mut cameras, 0, "camera.eye", &[0.0, 0.0, 1.0])?;
    put(&mut cameras, 0, "camera.target", &[0.0, 0.0, 0.0])?;
    put(&mut cameras, 0, "camera.up", &[0.0, 1.0, 0.0])?;
    put(&mut cameras, 0, "camera.span", &[1.0])?;
    put(&mut cameras, 0, "camera.near", &[0.1])?;
    put(&mut cameras, 0, "camera.far", &[100.0])?;
    let mut lights = Dataset::new(&code, 4, 1)?;
    word(&mut lights, 0, "node", 1)?;
    word(&mut lights, 0, "light.kind", 1)?;
    put(&mut lights, 0, "light.position", &[1.0, 0.0, 0.0])?;
    put(&mut lights, 0, "light.energy", &[1.0, 1.0, 1.0])?;
    let data = vec![
        nodes,
        worlds,
        cameras,
        Dataset::new(&code, 3, 1)?,
        lights,
        Dataset::new(&code, 5, 1)?,
    ];
    let start = Instant::now();
    let data = match backend {
        "cpu" => {
            let code = CpuProgram::new(code)?;
            let mut data = data;
            code.run(&mut data, &[])?;
            data
        }
        "gpu" => {
            let device = Device::for_program(&code)?;
            let data = data
                .into_iter()
                .map(|d| device.upload(d))
                .collect::<Result<Vec<_>>>()?;
            let code = device.compile(code)?;
            let done = code.submit(data, &[])?.wait()?;
            done.datasets()
                .iter()
                .map(|d| device.download(d))
                .collect::<Result<Vec<_>>>()?
        }
        _ => return Err(std::io::Error::other("后端须为 cpu 或 gpu").into()),
    };
    println!("setup_run_read_ms={:.3}", elapsed(start));
    for i in 0..3 {
        let d = &data[1];
        let stride = d.stride() as usize / 4;
        let translation = |field: &str| -> Result<f32> {
            Ok(f32::from_bits(
                d.words()[i * stride + d.field_offset(field)? as usize / 4 + 3],
            ))
        };
        println!(
            "node[{i}] world=({:.1},{:.1},{:.1})",
            translation("matrix.r0")?,
            translation("matrix.r1")?,
            translation("matrix.r2")?
        );
    }
    let camera = &data[3];
    let light = &data[5];
    let vector = |d: &Dataset, field: &str| -> Result<[f32; 3]> {
        let i = d.field_offset(field)? as usize / 4;
        Ok([
            f32_at(d, 0, field)?,
            f32::from_bits(d.words()[i + 1]),
            f32::from_bits(d.words()[i + 2]),
        ])
    };
    println!("camera.eye={:?}", vector(camera, "eye")?);
    println!("camera.up={:?}", vector(camera, "up")?);
    println!("light.position={:?}", vector(light, "position")?);
    Ok(())
}
