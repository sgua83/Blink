use blcompute::{Dataset, Value, cpu::CpuProgram};
use std::path::Path;

fn main() -> blcompute::Result<()> {
    let root = Path::new("sample/hybrid/Programs");
    let mut compiled = blanguage::compile_entry(
        &root.join("simulation/statistics.b"),
        root,
        "step_and_total",
    )?;
    let code = compiled.programs.remove(0);

    let mut bodies = Dataset::new(&code, 0, 3)?;
    let total = Dataset::new(&code, 1, 1)?;
    let stride = bodies.stride() as usize / 4;
    let position = bodies.field_offset("position")? as usize / 4;
    let velocity = bodies.field_offset("velocity")? as usize / 4;

    for (i, row) in bodies.words_mut().chunks_exact_mut(stride).enumerate() {
        row[position] = (1.0 + i as f32 * 0.001).to_bits();
        row[velocity + 1] = 1.0_f32.to_bits();
    }

    let mut data = vec![bodies, total];
    let values = [Value::F32(0.01), Value::F32(0.2)];
    let program = CpuProgram::new(code)?;
    program.run(&mut data, &values)?;

    let first_x = f32::from_bits(data[0].words()[position]);
    let first_y = f32::from_bits(data[0].words()[position + 1]);
    let energy = data[1].field_offset("value")? as usize / 4;
    let total_energy = f32::from_bits(data[1].words()[energy]);
    println!("first_x={first_x:.6} first_y={first_y:.6}");
    println!("total_energy={total_energy:.6}");
    Ok(())
}
