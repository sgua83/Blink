use blcompute::{Dataset, Value, vulkan::Device};
use std::path::Path;

fn main() -> blcompute::Result<()> {
    let root = Path::new("sample/hybrid/Programs");
    let mut compiled = blanguage::compile_entry(
        &root.join("simulation/statistics.b"), root, "step_and_total",
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
    // This application chooses GPU execution in code, without a CLI switch.
    let device = Device::for_program(&code)?;
    let program = device.compile(code)?;
    let inputs = vec![device.upload(bodies)?, device.upload(total)?];
    let values = [Value::F32(0.01), Value::F32(0.2)];
    let done = program.submit(inputs, &values)?.wait().map_err(|(error, _)| error)?;
    let bodies = device.download(&done.datasets()[0])?;
    let total = device.download(&done.datasets()[1])?;
    let energy = total.field_offset("value")? as usize / 4;
    println!("first_x={:.6} first_y={:.6}",
        f32::from_bits(bodies.words()[position]),
        f32::from_bits(bodies.words()[position + 1]));
    println!("total_energy={:.6}", f32::from_bits(total.words()[energy]));
    Ok(())
}
