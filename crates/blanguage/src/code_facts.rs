use std::{fs, path::Path};

pub fn print(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: bc code <entry.blbi> --observations <ranges.blcr> | bc code --image <deployment.blri> --observations <ranges.blcr>";
    let (image, source, material) = match args {
        [_, source, flag, material] if source != "--image" && flag == "--observations" => (false, source, material),
        [_, mode, source, flag, material] if mode == "--image" && flag == "--observations" => (true, source, material),
        _ => return Err(usage.into()),
    };
    let material = fs::read(material)?;
    let facts = if image {
        blanguage::build::image_code_facts(&fs::read(source)?, &material)
    } else {
        blanguage::build::code_facts(Path::new(source), &material)
    }.map_err(|e| e.to_string())?;
    println!("{facts}");
    Ok(())
}
