//! AI query of the artifact's original input record; no Compiler or source loading.
use std::path::Path;

pub fn print(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let report = blanguage::build::input_facts_json(path)
        .map_err(|error| -> Box<dyn std::error::Error> { error })?;
    println!("{report}");
    Ok(())
}
