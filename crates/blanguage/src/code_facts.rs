use std::{fs, path::Path};

pub fn print(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.get(1).is_some_and(|arg| arg == "--object") { return object(args); }
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

fn object(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use blanguage::object::{CodeSelection, code_facts};
    let (source, selection) = match args {
        [_, _, source] => (source, CodeSelection::CodeSections),
        [_, _, source, flag, number] if flag == "--section" => (source, CodeSelection::Section(number.parse()?)),
        [_, _, source, flag, name] if flag == "--function" => (source, CodeSelection::Function(name)),
        _ => return Err("usage: bc code --object <compiled.obj> [--section number | --function exact-symbol]".into()),
    };
    let facts = code_facts(&fs::read(source)?, selection)?;
    println!("{facts}");
    Ok(())
}
