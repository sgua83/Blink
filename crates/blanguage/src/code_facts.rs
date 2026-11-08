use std::{fs, path::Path};

pub fn print(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.get(1).is_some_and(|arg| arg == "--object") { return object(args); }
    let usage = "usage: bc code <entry.blbi> [--observations <ranges.blcr>] [--source-range start:end] | bc code --image <deployment.blri> [--observations <ranges.blcr>] [--source-range start:end]";
    let image = args.get(1).is_some_and(|value| value == "--image");
    let at = if image { 2 } else { 1 };
    let source = args.get(at).ok_or(usage)?;
    let mut at = at + 1; let mut material = None; let mut range = None;
    while at < args.len() {
        let value = args.get(at + 1).ok_or(usage)?;
        match args[at].as_str() {
            "--observations" if material.is_none() => material = Some(value),
            "--source-range" if range.is_none() => {
                let (start, end) = value.split_once(':').ok_or(usage)?;
                range = Some(start.parse()?..end.parse()?);
            }
            _ => return Err(usage.into()),
        }
        at += 2;
    }
    let material = material.map(fs::read).transpose()?;
    let facts = match (image, range) {
        (true, Some(range)) => blanguage::build::image_source_facts(&fs::read(source)?, range, material.as_deref()),
        (false, Some(range)) => blanguage::build::source_facts(Path::new(source), range, material.as_deref()),
        (true, None) => blanguage::build::image_code_facts(&fs::read(source)?, material.as_deref()),
        (false, None) => blanguage::build::code_facts(Path::new(source), material.as_deref()),
    }.map_err(|e| e.to_string())?;
    println!("{facts}");
    Ok(())
}

fn object(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    use blanguage::object::{CodeSelection, code_facts, code_facts_with_observations, source_facts};
    let usage = "usage: bc code --object <compiled.obj> [--observations <ranges.blcf>] [--section number | --function exact-symbol] [--source-range start:end]";
    let source = args.get(2).ok_or(usage)?;
    let mut selection = None; let mut material = None; let mut range = None; let mut at = 3;
    while at < args.len() {
        let value = args.get(at + 1).ok_or(usage)?;
        match args[at].as_str() {
            "--observations" if material.is_none() => material = Some(value),
            "--source-range" if range.is_none() => {
                let (start, end) = value.split_once(':').ok_or(usage)?;
                range = Some(start.parse()?..end.parse()?);
            }
            "--section" if selection.is_none() => selection = Some(CodeSelection::Section(value.parse()?)),
            "--function" if selection.is_none() => selection = Some(CodeSelection::Function(value)),
            _ => return Err(usage.into()),
        }
        at += 2;
    }
    let selection = selection.unwrap_or(CodeSelection::CodeSections);
    let bytes = fs::read(source)?;
    let material = material.map(fs::read).transpose()?;
    let facts = match (range, material.as_deref()) {
        (Some(range), material) => source_facts(&bytes, selection, range, material)?,
        (None, Some(material)) => code_facts_with_observations(&bytes, selection, material)?,
        (None, None) => code_facts(&bytes, selection)?,
    };
    println!("{facts}");
    Ok(())
}
