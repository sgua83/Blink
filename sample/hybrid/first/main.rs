#[allow(non_camel_case_types, dead_code)]
#[path = "../../../target/hybrid/numbers.rs"]
mod numbers;

fn main() -> Result<(), String> {
    let mut values: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
    let mut program = numbers::Program::new(numbers::Backend::Cpu)?;
    println!("before: {values:?}");
    program.run(&mut values)?;
    println!("after:  {values:?}");
    Ok(())
}
