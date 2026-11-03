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
    use blanguage::object::{CodeSelection, code_facts, code_facts_with_observations};
    let usage = "usage: bc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol]";
    let source = args.get(2).ok_or(usage)?;
    let mut selection = None; let mut material = None; let mut at = 3;
    while at < args.len() {
        let value = args.get(at + 1).ok_or(usage)?;
        match args[at].as_str() {
            "--observations" if material.is_none() => material = Some(value),
            "--section" if selection.is_none() => selection = Some(CodeSelection::Section(value.parse()?)),
            "--function" if selection.is_none() => selection = Some(CodeSelection::Function(value)),
            _ => return Err(usage.into()),
        }
        at += 2;
    }
    let selection = selection.unwrap_or(CodeSelection::CodeSections);
    let bytes = fs::read(source)?;
    let facts = match material {
        Some(path) => code_facts_with_observations(&bytes, selection, &fs::read(path)?)?,
        None => code_facts(&bytes, selection)?,
    };
    println!("{facts}");
    Ok(())
}
