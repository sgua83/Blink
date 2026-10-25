#[allow(non_camel_case_types, dead_code)]
#[path = "../../target/interop/map.rs"]
mod map;

fn main() -> Result<(), String> {
    let mut values = [1.0, 2.0, 3.0, 4.0];
    let mut program = map::Program::new(map::Backend::Cpu)?;
    program.run(&mut values)?;
    println!("{values:?}");
    Ok(())
}
