pub fn print(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let [_, source] = args else { return Err("usage: bc spirv <module.spv>".into()); };
    let bytes = std::fs::read(source)?;
    if bytes.len() % 4 != 0 { return Err(format!("SPIR-V file ends with an incomplete word at byte {}", bytes.len() / 4 * 4).into()); }
    let words = bytes.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect::<Vec<_>>();
    let facts = blanguage::spirv::module_json(&words)?;
    println!("{facts}"); Ok(())
}
